use crate::db::Db;
use crate::yt::Yt;
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
}

impl Core {
    pub fn download_dir(&self) -> PathBuf {
        self.db
            .get_setting("download_dir")
            .map(PathBuf::from)
            .unwrap_or_else(|| self.default_download_dir.clone())
    }

    pub fn ytdlp_dir(&self) -> PathBuf {
        self.data_dir.join("bin")
    }
}

#[cfg(test)]
pub fn test_core() -> std::sync::Arc<Core> {
    let dir = std::env::temp_dir().join("ytubem-test");
    std::fs::create_dir_all(&dir).unwrap();
    std::sync::Arc::new(Core {
        db: Db::open(&dir.join("test.db")).unwrap(),
        yt: Yt::new(dir.join("rp"), dir.join("bin")).unwrap(),
        http: reqwest::Client::new(),
        default_download_dir: dir.join("dl"),
        data_dir: dir,
        port: AtomicU16::new(0),
        download_slots: Semaphore::new(3),
        downloading: Default::default(),
    })
}
