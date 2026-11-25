//! Encrypted replacement for rustypipe's plaintext cache file. That cache holds the
//! YouTube session (login cookies) and visitor/OAuth data, i.e. account access.
//! Format: "YTS1" | nonce [12] | ChaCha20-Poly1305 ciphertext+tag (tampering is detected).
use crate::secrets::Key;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use rustypipe::cache::CacheStorage;
use std::path::PathBuf;

const MAGIC: &[u8; 4] = b"YTS1";
/// rustypipe's default plaintext cache file name (migrated, then deleted).
const LEGACY_FILE: &str = "rustypipe_cache.json";

pub struct SecureStorage {
    path: PathBuf,
    cipher: ChaCha20Poly1305,
}

impl SecureStorage {
    pub fn new(dir: PathBuf, key: &Key) -> Self {
        let s = SecureStorage { path: dir.join("session.ytsec"), cipher: ChaCha20Poly1305::new(key.into()) };
        s.migrate_legacy(&dir);
        s
    }

    /// Encrypt an existing plaintext cache from older versions, then remove it.
    fn migrate_legacy(&self, dir: &std::path::Path) {
        let legacy = dir.join(LEGACY_FILE);
        if let Ok(plain) = std::fs::read_to_string(&legacy) {
            if !self.path.exists() {
                self.write(&plain);
            }
            if self.path.exists() {
                let _ = std::fs::remove_file(&legacy);
            }
        }
    }

    fn seal(&self, data: &[u8]) -> Option<Vec<u8>> {
        let mut nonce = [0u8; 12];
        getrandom::fill(&mut nonce).ok()?;
        let ct = self.cipher.encrypt(Nonce::from_slice(&nonce), data).ok()?;
        let mut out = Vec::with_capacity(16 + ct.len());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ct);
        Some(out)
    }

    fn open(&self, blob: &[u8]) -> Option<Vec<u8>> {
        if blob.len() < 16 || &blob[..4] != MAGIC {
            return None;
        }
        self.cipher.decrypt(Nonce::from_slice(&blob[4..16]), &blob[16..]).ok()
    }
}

impl CacheStorage for SecureStorage {
    fn write(&self, data: &str) {
        let Some(blob) = self.seal(data.as_bytes()) else { return };
        // write-then-rename so a crash never leaves a half-written session file
        let tmp = self.path.with_extension("tmp");
        if std::fs::write(&tmp, &blob).is_ok() {
            let _ = std::fs::rename(&tmp, &self.path);
        }
    }

    fn read(&self) -> Option<String> {
        let blob = std::fs::read(&self.path).ok()?;
        String::from_utf8(self.open(&blob)?).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_tamper_and_migration() {
        let dir = std::env::temp_dir().join(format!("ytubem-secure-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(LEGACY_FILE), r#"{"auth_cookie":"SAPISID=secret"}"#).unwrap();
        let key = [9u8; 32];
        let s = SecureStorage::new(dir.clone(), &key);
        assert!(!dir.join(LEGACY_FILE).exists(), "plaintext cache removed");
        assert_eq!(s.read().unwrap(), r#"{"auth_cookie":"SAPISID=secret"}"#);
        let raw = std::fs::read(dir.join("session.ytsec")).unwrap();
        assert!(!String::from_utf8_lossy(&raw).contains("SAPISID"), "encrypted on disk");

        // a flipped byte must be rejected, not silently decrypted
        let mut bad = raw.clone();
        let last = bad.len() - 1;
        bad[last] ^= 1;
        std::fs::write(dir.join("session.ytsec"), &bad).unwrap();
        assert!(s.read().is_none());

        // wrong key can't read it
        s.write("x");
        assert!(SecureStorage::new(dir.clone(), &[1u8; 32]).read().is_none());
        std::fs::remove_dir_all(&dir).ok();
    }
}
