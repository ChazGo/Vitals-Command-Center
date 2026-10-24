mod google;
mod model;
mod oauth;
mod storage;

use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    ffi::{c_char, CStr, CString},
    sync::Mutex,
};

static OPERATION: Mutex<()> = Mutex::new(());

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "camelCase", deny_unknown_fields)]
enum Request {
    Status {},
    Configure {
        #[serde(rename = "clientId")]
        client_id: String,
        #[serde(rename = "clientSecret", default)]
        client_secret: String,
    },
    Connect {},
    Sync {
        days: u32,
    },
    Disconnect {},
}

fn status(store: &storage::Store) -> Value {
    json!({
        "configured": !store.client_id.is_empty(),
        "connected": store.tokens.is_some(),
        "clientId": store.client_id,
        "lastSync": store.last_sync,
        "days": store.days,
        "warnings": store.warnings,
    })
}

fn execute(request: &str) -> Result<Value, String> {
    let request: Request =
        serde_json::from_str(request).map_err(|_| "Invalid native backend request.")?;
    let _guard = OPERATION
        .lock()
        .map_err(|_| "The native backend must be restarted after an internal error.")?;
    let mut store = storage::load()?;
    match request {
        Request::Status {} => {}
        Request::Configure {
            client_id,
            client_secret,
        } => {
            let client_id = client_id.trim();
            if !client_id.ends_with(".apps.googleusercontent.com")
                || client_id.contains(char::is_whitespace)
                || client_id.len() > 512
                || client_secret.len() > 4096
            {
                return Err("Enter a valid Google Desktop OAuth client ID ending in .apps.googleusercontent.com.".into());
            }
            if client_id != store.client_id {
                store = storage::Store::default();
                store.client_id = client_id.to_string();
            }
            if !client_secret.is_empty() {
                store.client_secret = client_secret;
            }
            storage::save(&store)?;
        }
        Request::Connect {} => google::connect(&mut store)?,
        Request::Sync { days } => google::sync(&mut store, days)?,
        Request::Disconnect {} => {
            store.tokens = None;
            store.days.clear();
            store.last_sync = None;
            store.warnings.clear();
            storage::save(&store)?;
        }
    }
    Ok(status(&store))
}

fn response(input: &str) -> String {
    match execute(input) {
        Ok(data) => json!({"ok": true, "data": data}).to_string(),
        Err(error) => json!({"ok": false, "error": error}).to_string(),
    }
}

/// Execute a UTF-8 JSON request and return an owned NUL-terminated JSON response.
///
/// # Safety
/// `request` must point to a valid NUL-terminated string for the duration of this
/// call. The returned allocation must be released exactly once with `vitals_free`.
#[no_mangle]
pub unsafe extern "C" fn vitals_call(request: *const c_char) -> *mut c_char {
    let result = std::panic::catch_unwind(|| {
        if request.is_null() {
            return json!({"ok":false,"error":"Missing native backend request."}).to_string();
        }
        // The frontend provides a live NUL-terminated UTF-8 request.
        match unsafe { CStr::from_ptr(request) }.to_str() {
            Ok(input) if input.len() <= 16_384 => response(input),
            _ => json!({"ok":false,"error":"Native request is invalid UTF-8 or too large."})
                .to_string(),
        }
    })
    .unwrap_or_else(|_| {
        json!({"ok":false,"error":"Unexpected native backend error. Restart Vitals."}).to_string()
    });
    CString::new(result)
        .expect("JSON escapes embedded NUL characters")
        .into_raw()
}

/// Release an allocation returned by `vitals_call`.
///
/// # Safety
/// `result` must be null or a live pointer returned by `vitals_call`, not already freed.
#[no_mangle]
pub unsafe extern "C" fn vitals_free(result: *mut c_char) {
    if !result.is_null() {
        // Ownership returns to Rust exactly once.
        drop(unsafe { CString::from_raw(result) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_operations_and_invalid_json_are_rejected() {
        for input in [
            "garbage",
            r#"{"operation":"launch","target":"anything"}"#,
            r#"{"operation":"status","extra":1}"#,
        ] {
            let result: Value = serde_json::from_str(&response(input)).unwrap();
            assert_eq!(result["ok"], false);
        }
    }
    #[test]
    fn status_does_not_expose_oauth_secrets() {
        let store = storage::Store {
            client_secret: "sensitive-client-value".into(),
            tokens: Some(storage::Tokens {
                access_token: "secret-access".into(),
                refresh_token: "secret-refresh".into(),
                expires_at: 1,
            }),
            ..Default::default()
        };
        let output = status(&store).to_string();
        assert!(!output.contains("secret"));
        assert!(status(&store)["connected"].as_bool().unwrap());
    }
    #[test]
    fn ffi_null_request_returns_a_freeable_error() {
        let pointer = unsafe { vitals_call(std::ptr::null()) };
        let result: Value =
            serde_json::from_slice(unsafe { CStr::from_ptr(pointer) }.to_bytes()).unwrap();
        assert_eq!(result["ok"], false);
        unsafe { vitals_free(pointer) };
    }
}
