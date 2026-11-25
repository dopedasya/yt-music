//! Offline downloads: fetch the audio stream in bounded chunks and save it as is — the
//! original AAC (.m4a) or Opus (.webm) from YouTube, no re-encoding — named
//! "Artist - Title", so other players can open it too. Older versions stored encrypted
//! `.ytmc` files (see `vault`); those still play.
use crate::models::{DownloadProgress, Track};
use crate::state::Core;
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;
use std::sync::Arc;
use std::time::Instant;
use tauri::{AppHandle, Emitter};

const CHUNK: u64 = 4 * 1024 * 1024;

fn emit(app: &AppHandle, id: &str, state: &'static str, progress: f32, error: Option<String>) {
    let _ = app.emit(
        "download-progress",
        DownloadProgress { id: id.to_string(), state, progress, error },
    );
}

pub fn enqueue(app: AppHandle, core: Arc<Core>, tracks: Vec<Track>) {
    for t in tracks {
        {
            let mut busy = core.downloading.lock().unwrap();
            if core.db.download_path(&t.id).is_some() || !busy.insert(t.id.clone()) {
                continue;
            }
        }
        emit(&app, &t.id, "queued", 0.0, None);
        let (app, core) = (app.clone(), core.clone());
        tauri::async_runtime::spawn(async move {
            let _permit = core.download_slots.acquire().await;
            emit(&app, &t.id, "downloading", 0.0, None);
            let progress = |p: f32| emit(&app, &t.id, "downloading", p, None);
            let res = download_one(&core, &t, progress).await;
            core.downloading.lock().unwrap().remove(&t.id);
            match res {
                Ok(()) => emit(&app, &t.id, "done", 1.0, None),
                Err(e) => {
                    eprintln!("download {} failed: {e:#}", t.id);
                    emit(&app, &t.id, "error", 0.0, Some(format!("{}: {e:#}", t.title)));
                }
            }
        });
    }
}

/// `<dir>/<Artist> - <Title>.<ext>`; the video id is added if that name is taken.
pub fn file_for(dir: &Path, t: &Track, ext: &str) -> PathBuf {
    let artists = t.artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ");
    let base = safe_name(&if artists.is_empty() { t.title.clone() } else { format!("{artists} - {}", t.title) });
    let plain = dir.join(format!("{base}.{ext}"));
    if plain.exists() { dir.join(format!("{base} [{}].{ext}", t.id)) } else { plain }
}

/// A file name every OS accepts: no reserved characters, no trailing dots or spaces, not too long.
fn safe_name(s: &str) -> String {
    let s: String = s.chars().map(|c| if c.is_control() || r#"<>:"/\|?*"#.contains(c) { '_' } else { c }).collect();
    let s: String = s.chars().take(120).collect();
    let s = s.trim().trim_end_matches(['.', ' ']).to_string();
    if s.is_empty() { "track".into() } else { s }
}

async fn download_one(core: &Core, t: &Track, progress: impl Fn(f32) + Sync) -> Result<()> {
    let dir = core.download_dir();
    tokio::fs::create_dir_all(&dir).await.context("couldn't create the download folder")?;
    // googlevideo occasionally refuses a freshly issued URL (403); like the playback
    // proxy, drop the cached URL and try once more with a new one.
    let mut attempt = 0;
    let (info, dest) = loop {
        let info = core.yt.stream(&t.id, core.download_codec()).await?;
        let dest = file_for(&dir, t, if info.mime.contains("webm") { "webm" } else { "m4a" });
        match fetch_file(core, &info.url, info.size, &dest, &progress).await {
            Ok(()) => break (info, dest),
            Err(e) if attempt == 0 => {
                eprintln!("download {} attempt 1 failed ({e:#}); retrying with a fresh URL", t.id);
                core.yt.invalidate(&t.id);
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    };
    let size = tokio::fs::metadata(&dest).await?.len();
    core.db.add_download(t, &dest.to_string_lossy(), size, info.codec.label(), info.kbps)?;
    Ok(())
}

/// Once after updating from 1.1.0 or older: turn every encrypted `.ytmc` download into a
/// plain "Artist - Title" file in the download folder, so it plays in other players too.
/// A file that fails stays as it was (it still plays in the app) and is tried next start.
pub async fn decrypt_old_downloads(core: &Core) {
    let Ok(list) = core.db.downloads() else { return };
    let old: Vec<_> = list.into_iter().filter(|d| d.path.ends_with(&format!(".{}", crate::vault::EXT))).collect();
    if old.is_empty() {
        return;
    }
    let Ok(key) = core.vault_key() else { return };
    let dir = core.download_dir();
    if tokio::fs::create_dir_all(&dir).await.is_err() {
        return;
    }
    // oldest first: re-adding stamps "now", so the list keeps its order
    for d in old.into_iter().rev() {
        match decrypt_one(&key, &d.path, &dir, &d.track).await {
            Ok((dest, size)) => {
                let codec = d.codec.clone().unwrap_or_else(|| "AAC".into());
                if core.db.add_download(&d.track, &dest.to_string_lossy(), size, &codec, d.kbps).is_ok() {
                    let _ = tokio::fs::remove_file(&d.path).await;
                }
            }
            Err(e) => eprintln!("couldn't decrypt {}: {e:#}", d.path),
        }
    }
}

async fn decrypt_one(key: &crate::vault::Key, src: &str, dir: &Path, t: &Track) -> Result<(PathBuf, u64)> {
    use crate::vault;
    let mut file = tokio::fs::File::open(src).await?;
    let h = vault::read_header(&mut file).await?;
    let len = file.metadata().await?.len().saturating_sub(h.data_start);
    let dest = file_for(dir, t, if h.mime.contains("webm") { "webm" } else { "m4a" });
    let part = dest.with_extension("part");
    let mut out = tokio::fs::File::create(&part).await?;
    let mut r = vault::RangeReader::open(file, &h, key, 0, len).await?;
    while let Some(chunk) = r.next_chunk().await? {
        out.write_all(&chunk).await?;
    }
    out.sync_all().await?;
    drop(out);
    tokio::fs::rename(&part, &dest).await?;
    Ok((dest, len))
}

/// Download in bounded Range chunks (googlevideo rejects open-ended/huge ranges).
/// Writes to `.part` and renames on success.
async fn fetch_file(core: &Core, url: &str, size: Option<u64>, dest: &Path, progress: impl Fn(f32)) -> Result<()> {
    let part = dest.with_extension("part");
    let mut w = tokio::fs::File::create(&part).await?;
    let mut total = size;
    let mut done: u64 = 0;
    let mut last_emit = Instant::now();

    let result: Result<()> = async {
        loop {
            let end = done + CHUNK - 1;
            let end = total.map_or(end, |t| end.min(t.saturating_sub(1)));
            let resp = core
                .http
                .get(url)
                .header(reqwest::header::RANGE, format!("bytes={done}-{end}"))
                .send()
                .await?
                .error_for_status()?;
            if total.is_none() {
                total = resp
                    .headers()
                    .get(reqwest::header::CONTENT_RANGE)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.rsplit('/').next())
                    .and_then(|v| v.parse().ok());
            }
            let full_body = resp.status() == reqwest::StatusCode::OK; // server ignored Range
            let mut resp = resp;
            let mut got = 0u64;
            while let Some(chunk) = resp.chunk().await? {
                w.write_all(&chunk).await?;
                got += chunk.len() as u64;
                done += chunk.len() as u64;
                if last_emit.elapsed().as_millis() > 300 {
                    last_emit = Instant::now();
                    progress(total.map_or(0.0, |t| done as f32 / t as f32).min(0.99));
                }
            }
            if got == 0 || full_body || total.is_some_and(|t| done >= t) {
                break;
            }
        }
        if done == 0 {
            bail!("empty response");
        }
        if let Some(t) = total.filter(|t| done < *t) {
            bail!("incomplete download: {done} of {t} bytes");
        }
        Ok(())
    }
    .await;

    match result {
        Ok(()) => {
            w.flush().await?;
            w.sync_all().await?;
            drop(w);
            tokio::fs::rename(&part, dest).await?;
            Ok(())
        }
        Err(e) => {
            drop(w);
            let _ = tokio::fs::remove_file(&part).await;
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ArtistRef;

    #[tokio::test]
    async fn old_encrypted_download_becomes_a_plain_file() {
        let dir = std::env::temp_dir().join(format!("ytubem-dec-{}", crate::db::now()));
        std::fs::create_dir_all(&dir).unwrap();
        let key = crate::vault::Key::generate().unwrap();
        let src = dir.join("x.ytmc");
        let data: Vec<u8> = (0..700_000u32).map(|i| (i % 251) as u8).collect();
        let mut w = crate::vault::Writer::create(&src, &key, "audio/webm").await.unwrap();
        w.write(&data).await.unwrap();
        w.finish().await.unwrap();
        let t = Track { id: "id1".into(), title: "Song".into(), artists: vec![ArtistRef { id: None, name: "Band".into() }], album: None, duration: None, cover: None, plays: None };
        let (dest, size) = decrypt_one(&key, src.to_str().unwrap(), &dir, &t).await.unwrap();
        assert_eq!(dest.file_name().unwrap(), "Band - Song.webm");
        assert_eq!(size, data.len() as u64);
        assert_eq!(std::fs::read(&dest).unwrap(), data);
    }

    #[test]
    fn readable_safe_file_names() {
        let dir = std::env::temp_dir().join(format!("ytubem-names-{}", crate::db::now()));
        std::fs::create_dir_all(&dir).unwrap();
        let t = Track { id: "abc".into(), title: "What? / Why: *Now*.".into(), artists: vec![ArtistRef { id: None, name: "AC/DC".into() }], album: None, duration: None, cover: None, plays: None };
        let p = file_for(&dir, &t, "m4a");
        assert_eq!(p.file_name().unwrap(), "AC_DC - What_ _ Why_ _Now_.m4a");
        std::fs::write(&p, b"x").unwrap();
        assert_eq!(file_for(&dir, &t, "m4a").file_name().unwrap(), "AC_DC - What_ _ Why_ _Now_ [abc].m4a");
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;
    use crate::yt::Codec;

    #[tokio::test]
    #[ignore]
    async fn live_download_plain() {
        let core = crate::state::test_core();
        let t = core.yt.search("Okean Elzy Obiymy").await.unwrap().tracks.remove(0);
        let dir = core.download_dir();
        tokio::fs::create_dir_all(&dir).await.unwrap();
        for codec in [Codec::Aac, Codec::Opus] {
            let info = core.yt.stream(&t.id, codec).await.unwrap();
            let dest = dir.join(format!("test-{codec:?}"));
            fetch_file(&core, &info.url, info.size, &dest, |_| {}).await.unwrap();
            let head = std::fs::read(&dest).unwrap();
            match codec {
                Codec::Aac => assert_eq!(&head[4..8], b"ftyp", "MP4 signature"),
                Codec::Opus => assert_eq!(&head[..4], [0x1A, 0x45, 0xDF, 0xA3], "WebM/EBML signature"),
            }
            std::fs::remove_file(&dest).ok();
        }
    }
}
