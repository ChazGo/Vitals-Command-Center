use crate::{
    model::{self, Day, Metric, METRICS},
    oauth,
    storage::{self, Store, Tokens},
};
use chrono::{Local, NaiveDate, Utc};
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashSet},
    time::{Duration, Instant},
};

fn client() -> Result<Client, String> {
    Client::builder()
        .timeout(Duration::from_secs(25))
        .connect_timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("Health/0.1")
        .build()
        .map_err(|e| format!("Cannot initialize HTTPS: {e}"))
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: i64,
    scope: Option<String>,
}

fn token_request(params: &[(&str, &str)]) -> Result<TokenResponse, String> {
    let response = client()?
        .post("https://oauth2.googleapis.com/token")
        .form(params)
        .send()
        .map_err(|_| "Cannot reach Google's token service. Check your connection.")?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("Google token exchange failed (HTTP {}). Check your Desktop OAuth client details, consent/test-user configuration, or reconnect if authorization expired.", status.as_u16()));
    }
    let token: TokenResponse = response
        .json()
        .map_err(|_| "Google returned an invalid token response.")?;
    if token.access_token.is_empty() || token.expires_in <= 0 {
        return Err("Google returned an empty or expired access token.".into());
    }
    Ok(token)
}

pub fn connect(store: &mut Store) -> Result<(), String> {
    if store.client_id.is_empty() {
        return Err("Save a Google Desktop OAuth client ID in Settings first.".into());
    }
    let auth = oauth::authorize(&store.client_id)?;
    let mut params = vec![
        ("grant_type", "authorization_code"),
        ("code", &auth.code),
        ("client_id", &store.client_id),
        ("redirect_uri", &auth.redirect_uri),
        ("code_verifier", &auth.verifier),
    ];
    if !store.client_secret.is_empty() {
        params.push(("client_secret", &store.client_secret));
    }
    let response = token_request(&params)?;
    if let Some(scope) = &response.scope {
        let granted: HashSet<_> = scope.split_whitespace().collect();
        if !oauth::SCOPES
            .split_whitespace()
            .all(|s| granted.contains(s))
        {
            return Err("Not all three read-only Google Health permissions were granted. Reconnect and select fitness, health metrics, and sleep access.".into());
        }
    }
    let refresh_token = response.refresh_token.filter(|v| !v.is_empty()).ok_or(
        "Google did not issue offline access. Reconnect and approve the read-only permissions.",
    )?;
    store.tokens = Some(Tokens {
        access_token: response.access_token,
        refresh_token,
        expires_at: Utc::now().timestamp() + response.expires_in,
    });
    storage::save(store)
}

fn access_token(store: &mut Store) -> Result<String, String> {
    let tokens = store
        .tokens
        .as_ref()
        .ok_or("Connect your Google account before syncing.")?;
    if tokens.expires_at > Utc::now().timestamp() + 60 {
        return Ok(tokens.access_token.clone());
    }
    let mut params = vec![
        ("grant_type", "refresh_token"),
        ("refresh_token", &tokens.refresh_token),
        ("client_id", &store.client_id),
    ];
    if !store.client_secret.is_empty() {
        params.push(("client_secret", &store.client_secret));
    }
    let response = token_request(&params)?;
    let refresh = response
        .refresh_token
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| tokens.refresh_token.clone());
    let access = response.access_token.clone();
    store.tokens = Some(Tokens {
        access_token: response.access_token,
        refresh_token: refresh,
        expires_at: Utc::now().timestamp() + response.expires_in,
    });
    storage::save(store)?;
    Ok(access)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Page {
    #[serde(default)]
    data_points: Vec<Value>,
    #[serde(default)]
    next_page_token: String,
}

enum MetricResult {
    Points(Vec<Value>),
    Unavailable(String),
}

fn fetch_metric(
    http: &Client,
    token: &str,
    metric: &Metric,
    start: NaiveDate,
    end: NaiveDate,
    deadline: Instant,
) -> Result<MetricResult, String> {
    let url = format!(
        "https://health.googleapis.com/v4/users/me/dataTypes/{}/dataPoints:reconcile",
        metric.id
    );
    fetch_metric_at(http, token, metric, start, end, deadline, &url)
}

fn fetch_metric_at(
    http: &Client,
    token: &str,
    metric: &Metric,
    start: NaiveDate,
    end: NaiveDate,
    deadline: Instant,
    url: &str,
) -> Result<MetricResult, String> {
    let mut points = Vec::new();
    let mut page_token = String::new();
    let mut seen = HashSet::new();
    let filter = format!(
        "{} >= \"{start}\" AND {} < \"{end}\"",
        metric.filter_field, metric.filter_field
    );
    for _ in 0..100 {
        if Instant::now() > deadline {
            return Err("Sync exceeded its three-minute limit. Cached data was preserved; try a smaller date range.".into());
        }
        let page_size = if metric.id == "sleep" { "25" } else { "10000" };
        let response = http
            .get(url)
            .bearer_auth(token)
            .query(&[
                ("filter", filter.as_str()),
                ("pageSize", page_size),
                (
                    "dataSourceFamily",
                    "users/me/dataSourceFamilies/google-sources",
                ),
                ("pageToken", page_token.as_str()),
            ])
            .send()
            .map_err(|_| {
                format!(
                    "Cannot fetch {}. Check your connection; cached data was preserved.",
                    metric.id
                )
            })?;
        let status = response.status();
        if status.as_u16() == 403 || status.as_u16() == 404 {
            return Ok(MetricResult::Unavailable(format!("{} is unavailable (HTTP {}). Check Google Health API access, permissions and device support. Any older values were retained.", metric.id, status.as_u16())));
        }
        if !status.is_success() {
            return Err(format!(
                "Google Health {} failed (HTTP {}). No new snapshot was saved.{}",
                metric.id,
                status.as_u16(),
                if status.as_u16() == 401 {
                    " Reconnect your Google account."
                } else {
                    ""
                }
            ));
        }
        let page: Page = response.json().map_err(|_| {
            format!(
                "Google returned invalid {} JSON; cached data was preserved.",
                metric.id
            )
        })?;
        points.extend(page.data_points);
        if page.next_page_token.is_empty() {
            return Ok(MetricResult::Points(points));
        }
        if !seen.insert(page.next_page_token.clone()) {
            return Err(
                "Google repeated a pagination token; sync stopped without changing the snapshot."
                    .into(),
            );
        }
        page_token = page.next_page_token;
    }
    Err("Google Health pagination exceeded its safety limit; no new snapshot was saved.".into())
}

fn reset_metric(day: &mut Day, field: &str) {
    match field {
        "steps" => day.steps = None,
        "dailyRestingHeartRate" => day.resting_heart_rate = None,
        "dailyHeartRateVariability" => day.hrv = None,
        "dailyOxygenSaturation" => day.spo2 = None,
        "dailyRespiratoryRate" => day.respiratory_rate = None,
        "activeEnergyBurned" => day.active_calories = None,
        "sleep" => {
            day.sleep_hours = None;
            day.deep_hours = None;
            day.rem_hours = None;
            day.light_hours = None;
        }
        _ => {}
    }
}

pub fn sync(store: &mut Store, count: u32) -> Result<(), String> {
    if !(1..=90).contains(&count) {
        return Err("Choose a sync range between 1 and 90 days.".into());
    }
    let token = access_token(store)?;
    let today = Local::now().date_naive();
    let end = today + chrono::Duration::days(1);
    let start = end - chrono::Duration::days(count.into());
    let mut days: BTreeMap<_, _> = store
        .days
        .iter()
        .cloned()
        .map(|d| (d.date.clone(), d))
        .collect();
    let mut warnings = Vec::new();
    let mut fetched = 0;
    let http = client()?;
    let deadline = Instant::now() + Duration::from_secs(180);
    for metric in METRICS {
        match fetch_metric(&http, &token, metric, start, end, deadline)? {
            MetricResult::Unavailable(warning) => warnings.push(warning),
            MetricResult::Points(points) => {
                fetched += 1;
                for day in days.values_mut() {
                    if day.date >= start.to_string() && day.date < end.to_string() {
                        reset_metric(day, metric.field);
                    }
                }
                model::merge_points(metric, &points, &mut days, start, end)?;
            }
        }
    }
    if fetched == 0 {
        return Err(format!(
            "No Google Health metrics could be read. {}",
            warnings.join(" ")
        ));
    }
    store.days = days
        .into_values()
        .filter(|d| {
            d.steps.is_some()
                || d.resting_heart_rate.is_some()
                || d.hrv.is_some()
                || d.spo2.is_some()
                || d.respiratory_rate.is_some()
                || d.sleep_hours.is_some()
                || d.active_calories.is_some()
        })
        .collect();
    store.last_sync = Some(Utc::now().to_rfc3339());
    store.warnings = warnings;
    storage::save(store)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refresh_response_never_serializes_into_status() {
        let token: TokenResponse =
            serde_json::from_str(r#"{"access_token":"test","expires_in":3600}"#).unwrap();
        assert!(token.refresh_token.is_none());
        assert!(token.scope.is_none());
    }
    #[test]
    fn resetting_sleep_preserves_other_metrics() {
        let mut day = Day {
            steps: Some(100.0),
            sleep_hours: Some(6.0),
            deep_hours: Some(1.0),
            ..Day::default()
        };
        reset_metric(&mut day, "sleep");
        assert_eq!(day.steps, Some(100.0));
        assert_eq!(day.sleep_hours, None);
        assert_eq!(day.deep_hours, None);
    }

    fn fixture(
        replies: Vec<(u16, &'static str)>,
    ) -> (String, std::thread::JoinHandle<Vec<String>>) {
        use std::io::{BufRead, BufReader, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = format!(
            "http://{}/dataPoints:reconcile",
            listener.local_addr().unwrap()
        );
        let handle = std::thread::spawn(move || {
            let mut requests = Vec::new();
            for (status, body) in replies {
                let deadline = Instant::now() + Duration::from_secs(5);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(Instant::now() < deadline, "fixture request timed out");
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        Err(e) => panic!("fixture failed: {e}"),
                    }
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut request = String::new();
                let mut reader = BufReader::new(&stream);
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    request.push_str(&line);
                }
                requests.push(request);
                write!(stream, "HTTP/1.1 {status} Response\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
            }
            requests
        });
        (address, handle)
    }

    #[test]
    fn pagination_uses_reconciled_google_sources_and_follows_token() {
        let (url, server) = fixture(vec![
            (
                200,
                r#"{"dataPoints":[{"steps":{"count":"5"}}],"nextPageToken":"page-two"}"#,
            ),
            (200, r#"{"dataPoints":[{"steps":{"count":"7"}}]}"#),
        ]);
        let day = NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
        let result = fetch_metric_at(
            &client().unwrap(),
            "fixture-token",
            &METRICS[0],
            day,
            day + chrono::Duration::days(1),
            Instant::now() + Duration::from_secs(10),
            &url,
        )
        .unwrap();
        match result {
            MetricResult::Points(points) => assert_eq!(points.len(), 2),
            _ => panic!("fixture should return points"),
        }
        let requests = server.join().unwrap();
        assert!(requests[0]
            .contains("dataSourceFamily=users%2Fme%2FdataSourceFamilies%2Fgoogle-sources"));
        assert!(requests[0]
            .to_lowercase()
            .contains("authorization: bearer fixture-token"));
        assert!(requests[1].contains("pageToken=page-two"));
    }

    #[test]
    fn rate_limits_and_repeated_pages_are_explicit_errors() {
        let day = NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
        for replies in [
            vec![(429, "{}")],
            vec![
                (200, r#"{"nextPageToken":"same"}"#),
                (200, r#"{"nextPageToken":"same"}"#),
            ],
        ] {
            let (url, server) = fixture(replies);
            let result = fetch_metric_at(
                &client().unwrap(),
                "fixture-token",
                &METRICS[0],
                day,
                day + chrono::Duration::days(1),
                Instant::now() + Duration::from_secs(10),
                &url,
            );
            assert!(result.is_err());
            server.join().unwrap();
        }
    }

    #[test]
    fn permission_denial_is_a_visible_warning_not_fake_zeroes() {
        let (url, server) = fixture(vec![(403, "{}")]);
        let day = NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
        let result = fetch_metric_at(
            &client().unwrap(),
            "fixture-token",
            &METRICS[0],
            day,
            day + chrono::Duration::days(1),
            Instant::now() + Duration::from_secs(10),
            &url,
        )
        .unwrap();
        match result {
            MetricResult::Unavailable(warning) => assert!(warning.contains("403")),
            _ => panic!("permission denial must be reported"),
        }
        server.join().unwrap();
    }
}
