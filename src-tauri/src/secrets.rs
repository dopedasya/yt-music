//! 32-byte app keys kept outside the app's data files:
//! - Windows: Credential Manager (DPAPI-protected, per user)
//! - macOS: Keychain
//! - Linux / Android: a key file readable only by the owner (Android's app sandbox
//!   already isolates it from other apps).
//! So copying the data folder (library DB, offline `.ytmc` files, session cache) to another
//! machine or account doesn't hand over the keys needed to read it.
use anyhow::{Result, anyhow, bail};
use std::path::Path;

pub type Key = [u8; 32];

const SERVICE: &str = "io.github.neverh8me.ytmusic";

fn to_hex(k: &Key) -> String {
    k.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn from_hex(s: &str) -> Result<Key> {
    if s.len() != 64 {
        bail!("bad key length");
    }
    let mut k = [0u8; 32];
    for (i, b) in k.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)?;
    }
    Ok(k)
}

fn generate() -> Result<Key> {
    let mut k = [0u8; 32];
    getrandom::fill(&mut k).map_err(|e| anyhow!("rng: {e}"))?;
    Ok(k)
}

/// Get the key `name`, creating it if missing. `import` lets an existing key (e.g. one
/// that older versions kept in the database) be moved into the secret store once.
pub fn get_or_create(data_dir: &Path, name: &str, import: Option<Key>) -> Result<Key> {
    #[cfg(any(windows, target_os = "macos"))]
    {
        match os_store(name, import) {
            Ok(k) => return Ok(k),
            Err(e) => eprintln!("OS secret store unavailable ({e:#}); using key file"),
        }
    }
    file_store(data_dir, name, import)
}

#[cfg(any(windows, target_os = "macos"))]
fn os_store(name: &str, import: Option<Key>) -> Result<Key> {
    let entry = keyring::Entry::new(SERVICE, name)?;
    match entry.get_password() {
        Ok(hex) => from_hex(&hex),
        Err(keyring::Error::NoEntry) => {
            let k = match import {
                Some(k) => k,
                None => generate()?,
            };
            entry.set_password(&to_hex(&k))?;
            // read back so we never use a key that didn't actually persist
            let stored = from_hex(&entry.get_password()?)?;
            if stored != k {
                bail!("secret store did not persist the key");
            }
            Ok(k)
        }
        Err(e) => Err(e.into()),
    }
}

fn file_store(data_dir: &Path, name: &str, import: Option<Key>) -> Result<Key> {
    let path = data_dir.join(format!("{name}.key"));
    if let Ok(hex) = std::fs::read_to_string(&path) {
        return from_hex(hex.trim());
    }
    let k = match import {
        Some(k) => k,
        None => generate()?,
    };
    std::fs::create_dir_all(data_dir)?;
    std::fs::write(&path, to_hex(&k))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    let _ = SERVICE;
    Ok(k)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_store_roundtrip() {
        let dir = std::env::temp_dir().join(format!("ytubem-secrets-{}", std::process::id()));
        let a = file_store(&dir, "t", None).unwrap();
        assert_eq!(file_store(&dir, "t", None).unwrap(), a, "stable across calls");
        let imported = [7u8; 32];
        assert_eq!(file_store(&dir, "imp", Some(imported)).unwrap(), imported);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn hex() {
        let k = generate().unwrap();
        assert_eq!(from_hex(&to_hex(&k)).unwrap(), k);
    }
}
