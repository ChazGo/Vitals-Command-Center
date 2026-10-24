use crate::model::Day;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};
use zeroize::Zeroizing;

#[derive(Default, Serialize, Deserialize)]
pub struct Store {
    pub client_id: String,
    pub client_secret: String,
    pub tokens: Option<Tokens>,
    pub days: Vec<Day>,
    pub last_sync: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64,
}

fn path() -> Result<PathBuf, String> {
    let root = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is unavailable.")?;
    Ok(PathBuf::from(root).join("VitalsNative").join("state.dpapi"))
}

pub fn load() -> Result<Store, String> {
    let path = path()?;
    let encrypted = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Store::default()),
        Err(error) => return Err(format!("Cannot read local Vitals data: {error}")),
    };
    let plain = Zeroizing::new(crypt(&encrypted, false)?);
    serde_json::from_slice(&plain)
        .map_err(|_| "Local Vitals data is invalid; it has not been overwritten.".into())
}

pub fn save(store: &Store) -> Result<(), String> {
    let path = path()?;
    fs::create_dir_all(path.parent().ok_or("Invalid storage path.")?)
        .map_err(|e| format!("Cannot create local Vitals storage: {e}"))?;
    let plain = Zeroizing::new(
        serde_json::to_vec(store).map_err(|e| format!("Cannot encode local data: {e}"))?,
    );
    let encrypted = crypt(&plain, true)?;
    let temporary = path.with_extension("tmp");
    let mut file =
        fs::File::create(&temporary).map_err(|e| format!("Cannot save local data: {e}"))?;
    use std::io::Write;
    file.write_all(&encrypted)
        .and_then(|_| file.sync_all())
        .map_err(|e| format!("Cannot save local data: {e}"))?;
    drop(file);
    fs::rename(temporary, path).map_err(|e| format!("Cannot replace local data: {e}"))
}

#[cfg(windows)]
fn crypt(bytes: &[u8], protect: bool) -> Result<Vec<u8>, String> {
    use std::ptr::{null, null_mut};
    #[repr(C)]
    struct Blob {
        size: u32,
        data: *mut u8,
    }
    #[link(name = "crypt32")]
    extern "system" {
        fn CryptProtectData(
            input: *const Blob,
            description: *const u16,
            entropy: *const Blob,
            reserved: *mut std::ffi::c_void,
            prompt: *mut std::ffi::c_void,
            flags: u32,
            output: *mut Blob,
        ) -> i32;
        fn CryptUnprotectData(
            input: *const Blob,
            description: *mut *mut u16,
            entropy: *const Blob,
            reserved: *mut std::ffi::c_void,
            prompt: *mut std::ffi::c_void,
            flags: u32,
            output: *mut Blob,
        ) -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn LocalFree(memory: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    }
    let input = Blob {
        size: bytes
            .len()
            .try_into()
            .map_err(|_| "Local data is too large.")?,
        data: bytes.as_ptr() as *mut u8,
    };
    let mut output = Blob {
        size: 0,
        data: null_mut(),
    };
    // DPAPI receives valid live buffers and allocates its output with LocalAlloc.
    let result = unsafe {
        if protect {
            CryptProtectData(
                &input,
                null(),
                null(),
                null_mut(),
                null_mut(),
                1,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                null_mut(),
                null(),
                null_mut(),
                null_mut(),
                1,
                &mut output,
            )
        }
    };
    if result == 0 {
        return Err(format!(
            "Windows could not {} local data: {}",
            if protect { "encrypt" } else { "decrypt" },
            std::io::Error::last_os_error()
        ));
    }
    // Copy before releasing the Windows-owned buffer; clear decrypted output first.
    let copied = unsafe {
        let buffer = std::slice::from_raw_parts_mut(output.data, output.size as usize);
        let copied = buffer.to_vec();
        if !protect {
            use zeroize::Zeroize;
            buffer.zeroize();
        }
        LocalFree(output.data.cast());
        copied
    };
    Ok(copied)
}

#[cfg(not(windows))]
fn crypt(_: &[u8], _: bool) -> Result<Vec<u8>, String> {
    Err("Encrypted storage requires Windows DPAPI.".into())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn dpapi_round_trip_and_tampering_rejected() {
        let clear = b"local test data, not a real token";
        let encrypted = crypt(clear, true).unwrap();
        assert!(!encrypted.windows(clear.len()).any(|part| part == clear));
        assert_eq!(crypt(&encrypted, false).unwrap(), clear);
        let mut damaged = encrypted;
        damaged[0] ^= 0xff;
        assert!(crypt(&damaged, false).is_err());
    }
}
