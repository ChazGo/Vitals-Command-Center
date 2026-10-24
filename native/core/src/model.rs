use chrono::{DateTime, NaiveDate};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Default, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Day {
    pub date: String,
    pub steps: Option<f64>,
    pub resting_heart_rate: Option<f64>,
    pub hrv: Option<f64>,
    pub spo2: Option<f64>,
    pub respiratory_rate: Option<f64>,
    pub sleep_hours: Option<f64>,
    pub deep_hours: Option<f64>,
    pub rem_hours: Option<f64>,
    pub light_hours: Option<f64>,
    pub active_calories: Option<f64>,
}

pub struct Metric {
    pub id: &'static str,
    pub field: &'static str,
    pub filter_field: &'static str,
}

pub const METRICS: &[Metric] = &[
    Metric {
        id: "steps",
        field: "steps",
        filter_field: "steps.interval.civil_start_time",
    },
    Metric {
        id: "daily-resting-heart-rate",
        field: "dailyRestingHeartRate",
        filter_field: "dailyRestingHeartRate.date",
    },
    Metric {
        id: "daily-heart-rate-variability",
        field: "dailyHeartRateVariability",
        filter_field: "dailyHeartRateVariability.date",
    },
    Metric {
        id: "daily-oxygen-saturation",
        field: "dailyOxygenSaturation",
        filter_field: "dailyOxygenSaturation.date",
    },
    Metric {
        id: "daily-respiratory-rate",
        field: "dailyRespiratoryRate",
        filter_field: "dailyRespiratoryRate.date",
    },
    Metric {
        id: "sleep",
        field: "sleep",
        filter_field: "sleep.interval.civil_end_time",
    },
    Metric {
        id: "active-energy-burned",
        field: "activeEnergyBurned",
        filter_field: "active_energy_burned.interval.civil_start_time",
    },
];

fn number(value: &Value, field: &str) -> Result<f64, String> {
    let n = value
        .get(field)
        .and_then(|v| v.as_f64().or_else(|| v.as_str()?.parse().ok()))
        .filter(|n| n.is_finite() && *n >= 0.0);
    n.ok_or_else(|| format!("Google returned an invalid or missing {field} value."))
}

fn calendar_date(value: &Value) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(
        value.get("year")?.as_i64()?.try_into().ok()?,
        value.get("month")?.as_u64()?.try_into().ok()?,
        value.get("day")?.as_u64()?.try_into().ok()?,
    )
}

fn point_date(value: &Value, sleep: bool) -> Result<String, String> {
    if let Some(date) = value.get("date").and_then(calendar_date) {
        return Ok(date.to_string());
    }
    let interval = &value["interval"];
    let civil = if sleep {
        "civilEndTime"
    } else {
        "civilStartTime"
    };
    if let Some(date) = interval
        .get(civil)
        .and_then(|v| v.get("date"))
        .and_then(calendar_date)
    {
        return Ok(date.to_string());
    }
    let physical = if sleep { "endTime" } else { "startTime" };
    let timestamp = interval
        .get(physical)
        .and_then(Value::as_str)
        .ok_or("Google returned a data point without its observation date.")?;
    let parsed = DateTime::parse_from_rfc3339(timestamp)
        .map_err(|_| "Google returned an invalid observation timestamp.")?;
    let offset_field = if sleep {
        "endUtcOffset"
    } else {
        "startUtcOffset"
    };
    let date = match interval.get(offset_field).and_then(Value::as_str) {
        Some(offset) => {
            let seconds: f64 = offset
                .trim_end_matches('s')
                .parse()
                .map_err(|_| "Google returned an invalid observation UTC offset.")?;
            if !seconds.is_finite() || seconds.abs() > 86_400.0 {
                return Err("Google returned an out-of-range observation UTC offset.".into());
            }
            (parsed.naive_utc() + chrono::Duration::seconds(seconds as i64)).date()
        }
        None => parsed.date_naive(),
    };
    Ok(date.to_string())
}

fn add(slot: &mut Option<f64>, value: f64) {
    *slot = Some(slot.unwrap_or(0.0) + value);
}

pub fn merge_points(
    metric: &Metric,
    points: &[Value],
    days: &mut BTreeMap<String, Day>,
    start: NaiveDate,
    end: NaiveDate,
) -> Result<(), String> {
    for point in points {
        let value = point
            .get(metric.field)
            .ok_or_else(|| format!("Google's {} response has an unexpected schema.", metric.id))?;
        let date = point_date(value, metric.field == "sleep")?;
        let parsed = NaiveDate::parse_from_str(&date, "%Y-%m-%d")
            .map_err(|_| "Google returned an invalid date.")?;
        if parsed < start || parsed >= end {
            continue;
        }
        let day = days.entry(date.clone()).or_insert_with(|| Day {
            date,
            ..Day::default()
        });
        match metric.field {
            "steps" => add(&mut day.steps, number(value, "count")?),
            "dailyRestingHeartRate" => {
                day.resting_heart_rate = Some(number(value, "beatsPerMinute")?)
            }
            "dailyHeartRateVariability" => {
                day.hrv = Some(number(value, "averageHeartRateVariabilityMilliseconds")?)
            }
            "dailyOxygenSaturation" => day.spo2 = Some(number(value, "averagePercentage")?),
            "dailyRespiratoryRate" => {
                day.respiratory_rate = Some(number(value, "breathsPerMinute")?)
            }
            "activeEnergyBurned" => add(&mut day.active_calories, number(value, "kcal")?),
            "sleep" => merge_sleep(value, day)?,
            _ => return Err("Unsupported Google Health metric.".into()),
        }
    }
    Ok(())
}

fn merge_sleep(value: &Value, day: &mut Day) -> Result<(), String> {
    let summary = value
        .get("summary")
        .ok_or("Google returned sleep without a summary.")?;
    add(
        &mut day.sleep_hours,
        number(summary, "minutesAsleep")? / 60.0,
    );
    if let Some(stages) = value.get("stages").and_then(Value::as_array) {
        for stage in stages {
            let parse_time = |key: &str| -> Result<DateTime<chrono::FixedOffset>, String> {
                DateTime::parse_from_rfc3339(
                    stage[key]
                        .as_str()
                        .ok_or("Sleep stage is missing a timestamp.")?,
                )
                .map_err(|_| "Google returned an invalid sleep stage timestamp.".into())
            };
            let hours =
                (parse_time("endTime")? - parse_time("startTime")?).num_seconds() as f64 / 3600.0;
            if hours < 0.0 {
                return Err("Google returned a negative sleep stage duration.".into());
            }
            match stage["type"].as_str() {
                Some("DEEP") => add(&mut day.deep_hours, hours),
                Some("REM") => add(&mut day.rem_hours, hours),
                Some("LIGHT") => add(&mut day.light_hours, hours),
                _ => {}
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn range() -> (NaiveDate, NaiveDate) {
        (
            NaiveDate::from_ymd_opt(2026, 10, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
        )
    }
    #[test]
    fn string_counts_sum_and_missing_metrics_remain_absent() {
        let mut days = BTreeMap::new();
        let (start, end) = range();
        let p = json!({"steps":{"count":"250","interval":{"civilStartTime":{"date":{"year":2026,"month":10,"day":23}}}}});
        merge_points(&METRICS[0], &[p.clone(), p], &mut days, start, end).unwrap();
        let day = &days["2026-10-23"];
        assert_eq!(day.steps, Some(500.0));
        assert_eq!(day.hrv, None);
    }
    #[test]
    fn daily_schema_is_not_confused_with_legacy_fitbit() {
        let mut days = BTreeMap::new();
        let (start, end) = range();
        let p = json!({"dailyHeartRateVariability":{"date":{"year":2026,"month":10,"day":23},"averageHeartRateVariabilityMilliseconds":42.5}});
        merge_points(&METRICS[2], &[p], &mut days, start, end).unwrap();
        assert_eq!(days["2026-10-23"].hrv, Some(42.5));
    }
    #[test]
    fn sleep_uses_wake_date_and_excludes_awake_time() {
        let mut days = BTreeMap::new();
        let (start, end) = range();
        let p = json!({"sleep":{"interval":{"endTime":"2026-10-24T06:30:00Z","endUtcOffset":"-25200s"},"summary":{"minutesAsleep":"360"},"stages":[{"type":"DEEP","startTime":"2026-10-24T02:00:00Z","endTime":"2026-10-24T03:00:00Z"}]}});
        merge_points(&METRICS[5], &[p], &mut days, start, end).unwrap();
        assert_eq!(days["2026-10-23"].sleep_hours, Some(6.0));
        assert_eq!(days["2026-10-23"].deep_hours, Some(1.0));
    }
    #[test]
    fn invalid_values_and_unrecognized_shapes_fail_explicitly() {
        let mut days = BTreeMap::new();
        let (start, end) = range();
        assert!(merge_points(
            &METRICS[0],
            &[json!({"steps":{"count":"-5","date":{"year":2026,"month":10,"day":23}}})],
            &mut days,
            start,
            end
        )
        .is_err());
        assert!(merge_points(
            &METRICS[0],
            &[json!({"unexpected":1})],
            &mut days,
            start,
            end
        )
        .is_err());
    }
    #[test]
    fn outside_window_does_not_pollute_cache() {
        let mut days = BTreeMap::new();
        let (start, end) = range();
        let p = json!({"steps":{"count":"1","date":{"year":2025,"month":10,"day":23}}});
        merge_points(&METRICS[0], &[p], &mut days, start, end).unwrap();
        assert!(days.is_empty());
    }
}
