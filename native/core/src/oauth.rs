use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};
use url::Url;

pub const SCOPES: &str = "https://www.googleapis.com/auth/googlehealth.health_metrics_and_measurements.readonly https://www.googleapis.com/auth/googlehealth.sleep.readonly https://www.googleapis.com/auth/googlehealth.activity_and_fitness.readonly";

fn random_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

pub struct Authorization {
    pub code: String,
    pub verifier: String,
    pub redirect_uri: String,
}

pub fn authorize(client_id: &str) -> Result<Authorization, String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| format!("Cannot start local sign-in callback: {e}"))?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let redirect_uri = format!(
        "http://127.0.0.1:{}/oauth/callback",
        listener.local_addr().map_err(|e| e.to_string())?.port()
    );
    let state = random_token();
    let verifier = random_token();
    let mut url =
        Url::parse("https://accounts.google.com/o/oauth2/v2/auth").map_err(|e| e.to_string())?;
    url.query_pairs_mut().extend_pairs([
        ("client_id", client_id),
        ("redirect_uri", &redirect_uri),
        ("response_type", "code"),
        ("scope", SCOPES),
        ("access_type", "offline"),
        ("prompt", "consent"),
        ("state", &state),
        ("code_challenge", &challenge(&verifier)),
        ("code_challenge_method", "S256"),
    ]);
    open_browser(url.as_str())?;
    let deadline = Instant::now() + Duration::from_secs(180);
    while Instant::now() < deadline {
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .map_err(|e| e.to_string())?;
                let mut first_line = Vec::new();
                let read = BufReader::new((&stream).take(8192)).read_until(b'\n', &mut first_line);
                if read.is_err() || first_line.len() >= 8192 {
                    continue;
                }
                let line =
                    String::from_utf8(first_line).map_err(|_| "Invalid local sign-in request.")?;
                let mut parts = line.split_whitespace();
                let method = parts.next().unwrap_or("");
                let target = parts.next().unwrap_or("");
                if method != "GET" || !target.starts_with("/oauth/callback?") {
                    let _ = stream.write_all(
                        b"HTTP/1.1 404 Not Found\r\nConnection: close\r\nContent-Length: 0\r\n\r\n",
                    );
                    continue;
                }
                let result = callback_code(target, &state);
                let message = if result.is_ok() {
                    "Authorization received. Return to Health to finish connecting."
                } else {
                    "Sign-in was rejected. Return to Health for details."
                };
                let response = format!("HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{message}", message.len());
                stream
                    .write_all(response.as_bytes())
                    .map_err(|e| format!("Cannot complete browser callback: {e}"))?;
                return Ok(Authorization {
                    code: result?,
                    verifier,
                    redirect_uri,
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100))
            }
            Err(error) => return Err(format!("Local sign-in callback failed: {error}")),
        }
    }
    Err("Google sign-in timed out after three minutes. Try Connect again.".into())
}

fn callback_code(target: &str, expected: &str) -> Result<String, String> {
    let url = Url::parse(&format!("http://127.0.0.1{target}"))
        .map_err(|_| "Invalid sign-in callback.")?;
    let values: Vec<_> = url.query_pairs().collect();
    let state: Vec<_> = values.iter().filter(|(key, _)| key == "state").collect();
    if state.len() != 1 || state[0].1 != expected {
        return Err("Sign-in state did not match. No authorization was accepted.".into());
    }
    if values.iter().any(|(key, _)| key == "error") {
        return Err("Google sign-in was declined or failed. Try again when ready.".into());
    }
    let codes: Vec<_> = values.iter().filter(|(key, _)| key == "code").collect();
    if codes.len() != 1 || codes[0].1.is_empty() {
        return Err("Google did not return one authorization code.".into());
    }
    Ok(codes[0].1.to_string())
}

#[cfg(windows)]
fn open_browser(url: &str) -> Result<(), String> {
    #[link(name = "shell32")]
    extern "system" {
        fn ShellExecuteW(
            window: *mut std::ffi::c_void,
            operation: *const u16,
            file: *const u16,
            parameters: *const u16,
            directory: *const u16,
            show: i32,
        ) -> isize;
    }
    let wide: Vec<_> = url.encode_utf16().chain(Some(0)).collect();
    let open: Vec<_> = "open".encode_utf16().chain(Some(0)).collect();
    // Both strings are NUL-terminated, live for the call, and no shell command is constructed.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            open.as_ptr(),
            wide.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
        )
    };
    if result <= 32 {
        Err(format!(
            "Windows could not open your default browser (code {result})."
        ))
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn open_browser(_: &str) -> Result<(), String> {
    Err("Google sign-in requires Windows.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pkce_matches_rfc7636_vector() {
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }
    #[test]
    fn rejects_wrong_or_duplicate_state_and_google_denial() {
        assert!(callback_code("/oauth/callback?code=abc&state=wrong", "right").is_err());
        assert!(
            callback_code("/oauth/callback?code=abc&state=right&state=right", "right").is_err()
        );
        assert!(callback_code("/oauth/callback?error=access_denied&state=right", "right").is_err());
        assert_eq!(
            callback_code("/oauth/callback?code=abc&state=right", "right").unwrap(),
            "abc"
        );
    }
}
