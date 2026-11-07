use crate::db::Db;
use crate::player::Queue;
use crate::vault;
use crate::yt::{Codec, Yt};
use anyhow::Result;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::AtomicU16;
use tokio::sync::Semaphore;

pub struct Core {
    pub db: Db,
    pub yt: Yt,
    pub http: reqwest::Client,
    pub data_dir: PathBuf,
    pub default_download_dir: PathBuf,
    pub port: AtomicU16,
    pub download_slots: Semaphore,
    pub downloading: Mutex<HashSet<String>>,
    pub queue: tokio::sync::Mutex<Queue>,
    vault_key: Mutex<Option<vault::Key>>,
}

impl Core {
    pub fn new(db: Db, yt: Yt, http: reqwest::Client, data_dir: PathBuf, default_download_dir: PathBuf) -> Self {
        let queue = db
            .get_setting("queue")
            .and_then(|s| serde_json::from_str::<Queue>(&s).ok())
            .unwrap_or_else(Queue::new);
        Core {
            db,
            yt,
            http,
            data_dir,
            default_download_dir,
            port: AtomicU16::new(0),
            download_slots: Semaphore::new(3),
            downloading: Default::default(),
            queue: tokio::sync::Mutex::new(queue),
            vault_key: Mutex::new(None),
        }
    }

    pub fn download_dir(&self) -> PathBuf {
        self.db
            .get_setting("download_dir")
            .map(PathBuf::from)
            .unwrap_or_else(|| self.default_download_dir.clone())
    }

    /// Opus by default (better quality); AAC on request.
    pub fn download_codec(&self) -> Codec {
        match self.db.get_setting("download_quality").as_deref() {
            Some("aac") => Codec::Aac,
            _ => Codec::Opus,
        }
    }

    pub fn ytdlp_dir(&self) -> PathBuf {
        self.data_dir.join("bin")
    }

    /// Per-install key for the offline vault, created on first use.
    pub fn vault_key(&self) -> Result<vault::Key> {
        let mut slot = self.vault_key.lock().unwrap();
        if let Some(k) = slot.as_ref() {
            return Ok(k.clone());
        }
        let key = match self.db.get_setting("vault_key") {
            Some(hex) => vault::Key::from_hex(&hex)?,
            None => {
                let k = vault::Key::generate()?;
                self.db.set_setting("vault_key", &k.to_hex())?;
                k
            }
        };
        *slot = Some(key.clone());
        Ok(key)
    }
}

#[cfg(test)]
pub fn test_core() -> std::sync::Arc<Core> {
    let dir = std::env::temp_dir().join("ytubem-test");
    std::fs::create_dir_all(&dir).unwrap();
    std::sync::Arc::new(Core::new(
        Db::open(&dir.join("test.db")).unwrap(),
        Yt::new(dir.join("rp"), dir.join("bin")).unwrap(),
        reqwest::Client::new(),
        dir.clone(),
        dir.join("dl"),
    ))
}
