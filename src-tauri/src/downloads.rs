//! Offline downloads: fetch the audio stream in chunks, tag it, register it in the library.
use crate::models::{DownloadProgress, Track};
use crate::state::Core;
use crate::ytdlp;
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;

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
            let res = download_one(&app, &core, &t).await;
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

fn sanitize(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| if "<>:\"/\\|?*".contains(c) || c.is_control() { '_' } else { c })
        .collect();
    let trimmed = cleaned.trim().trim_end_matches(['.', ' ']);
    trimmed.chars().take(120).collect()
}

fn target_stem(dir: &Path, t: &Track) -> PathBuf {
    let artist = t.artist_line();
    let base = if artist.is_empty() {
        sanitize(&t.title)
    } else {
        sanitize(&format!("{artist} - {}", t.title))
    };
    let base = if base.is_empty() { t.id.clone() } else { base };
    let stem = dir.join(&base);
    // Same name for a different track (remixes, re-uploads) gets the id appended.
    let taken = ["m4a", "webm", "opus"].iter().any(|e| stem.with_extension(e).exists());
    if taken { dir.join(format!("{base} [{}]", t.id)) } else { stem }
}

async fn download_one(app: &AppHandle, core: &Core, t: &Track) -> Result<()> {
    let dir = core.download_dir();
    tokio::fs::create_dir_all(&dir).await.context("couldn't create the download folder")?;
    let stem = target_stem(&dir, t);

    let path = match core.yt.stream(&t.id).await {
        Ok(info) => {
            let ext = if info.mime.contains("webm") { "webm" } else { "m4a" };
            let dest = stem.with_extension(ext);
            let progress = |p: f32| emit(app, &t.id, "downloading", p, None);
            match fetch(core, &info.url, info.size, &dest, progress).await {
                Ok(()) => dest,
                Err(e) => {
                    eprintln!("direct download failed ({e:#}), using yt-dlp");
                    via_ytdlp(core, t, &stem).await?
                }
            }
        }
        Err(e) => {
            eprintln!("stream lookup failed ({e:#}), using yt-dlp");
            via_ytdlp(core, t, &stem).await?
        }
    };

    if path.extension().is_some_and(|e| e == "m4a")
        && let Err(e) = tag(core, t, &path).await
    {
        eprintln!("tagging {} failed: {e:#}", path.display());
    }
    let size = tokio::fs::metadata(&path).await?.len();
    core.db.add_download(t, &path.to_string_lossy(), size)?;
    Ok(())
}

async fn via_ytdlp(core: &Core, t: &Track, stem: &Path) -> Result<PathBuf> {
    let dest = stem.with_extension("m4a");
    ytdlp::download(&core.ytdlp_dir(), &t.id, &dest).await?;
    if !dest.is_file() {
        bail!("yt-dlp didn't produce a file");
    }
    Ok(dest)
}

async fn fetch(core: &Core, url: &str, size: Option<u64>, dest: &Path, progress: impl Fn(f32)) -> Result<()> {
    let part = dest.with_extension("part");
    let mut file = tokio::fs::File::create(&part).await?;
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
                file.write_all(&chunk).await?;
                got += chunk.len() as u64;
                done += chunk.len() as u64;
                if last_emit.elapsed().as_millis() > 300 {
                    last_emit = Instant::now();
                    let p = total.map_or(0.0, |t| done as f32 / t as f32);
                    progress(p.min(0.99));
                }
            }
            if got == 0 || full_body || total.is_some_and(|t| done >= t) {
                break;
            }
        }
        file.flush().await?;
        if done == 0 {
            bail!("empty response");
        }
        Ok(())
    }
    .await;

    drop(file);
    if let Err(e) = result {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(e);
    }
    tokio::fs::rename(&part, dest).await?;
    Ok(())
}

async fn tag(core: &Core, t: &Track, path: &Path) -> Result<()> {
    let cover = match &t.cover {
        Some(u) => match core.http.get(u).send().await {
            Ok(r) if r.status().is_success() => r.bytes().await.ok().map(|b| b.to_vec()),
            _ => None,
        },
        None => None,
    };
    let (t, path) = (t.clone(), path.to_path_buf());
    tokio::task::spawn_blocking(move || write_tags(&t, &path, cover)).await?
}

fn write_tags(t: &Track, path: &Path, cover: Option<Vec<u8>>) -> Result<()> {
    use lofty::config::WriteOptions;
    use lofty::picture::{MimeType, Picture, PictureType};
    use lofty::prelude::*;
    use lofty::tag::{Tag, TagType};

    let mut tagged = lofty::read_from_path(path)?;
    if tagged.primary_tag().is_none() {
        tagged.insert_tag(Tag::new(TagType::Mp4Ilst));
    }
    let tag = tagged.primary_tag_mut().context("no tag")?;
    tag.set_title(t.title.clone());
    let artist = t.artist_line();
    if !artist.is_empty() {
        tag.set_artist(artist);
    }
    if let Some(a) = &t.album {
        tag.set_album(a.name.clone());
    }
    tag.insert_text(ItemKey::Comment, format!("https://music.youtube.com/watch?v={}", t.id));
    if let Some(bytes) = cover {
        let mime = if bytes.starts_with(&[0x89, b'P', b'N', b'G']) { MimeType::Png } else { MimeType::Jpeg };
        tag.push_picture(Picture::unchecked(bytes).pic_type(PictureType::CoverFront).mime_type(mime).build());
    }
    tag.save_to_path(path, WriteOptions::default())?;
    Ok(())
}

#[cfg(test)]
mod live_tests {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn live_download_and_tag() {
        use lofty::prelude::*;
        let core = crate::state::test_core();
        let t = core.yt.search("Okean Elzy Obiymy").await.unwrap().tracks.remove(0);
        let dir = core.download_dir();
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let info = core.yt.stream(&t.id).await.unwrap();
        let dest = target_stem(&dir, &t).with_extension("m4a");
        let start = Instant::now();
        fetch(&core, &info.url, info.size, &dest, |_| {}).await.unwrap();
        println!("downloaded {} in {:?}", dest.display(), start.elapsed());
        tag(&core, &t, &dest).await.unwrap();
        let f = lofty::read_from_path(&dest).unwrap();
        let tg = f.primary_tag().unwrap();
        println!("title={:?} artist={:?} album={:?} pictures={} duration={:?}",
            tg.title(), tg.artist(), tg.album(), tg.pictures().len(), f.properties().duration());
        assert_eq!(tg.title().as_deref(), Some(t.title.as_str()));
        std::fs::remove_file(&dest).ok();
    }
}
