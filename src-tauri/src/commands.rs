//! Tauri commands invoked from the frontend.
use crate::models::*;
use crate::state::Core;
use crate::{downloads, ytdlp};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, State};

type Res<T> = Result<T, String>;
type St<'a> = State<'a, Arc<Core>>;

fn e<E: std::fmt::Display>(err: E) -> String {
    format!("{err:#}")
}

#[tauri::command]
pub fn stream_base(core: St) -> String {
    format!("http://127.0.0.1:{}", core.port.load(Ordering::Relaxed))
}

// ------------------------------------------------------------- YouTube Music
#[tauri::command]
pub async fn search(core: St<'_>, query: String) -> Res<SearchResults> {
    core.yt.search(&query).await.map_err(e)
}

#[tauri::command]
pub async fn search_suggestions(core: St<'_>, query: String) -> Res<Vec<String>> {
    core.yt.suggestions(&query).await.map_err(e)
}

#[tauri::command]
pub async fn home(core: St<'_>) -> Res<Home> {
    core.yt.home().await.map_err(e)
}

#[tauri::command]
pub async fn get_album(core: St<'_>, id: String) -> Res<AlbumPage> {
    core.yt.album(&id).await.map_err(e)
}

#[tauri::command]
pub async fn get_artist(core: St<'_>, id: String) -> Res<ArtistPage> {
    core.yt.artist(&id).await.map_err(e)
}

#[tauri::command]
pub async fn get_playlist(core: St<'_>, id: String) -> Res<PlaylistPage> {
    let authed = core.db.get_setting("logged_in").is_some();
    match core.yt.playlist(&id, authed).await {
        Ok(p) => Ok(p),
        // private playlists need auth, public ones sometimes fail with it: try the other way
        Err(first) => core.yt.playlist(&id, !authed).await.map_err(|_| e(first)),
    }
}

#[tauri::command]
pub async fn get_radio(core: St<'_>, id: String) -> Res<Vec<Track>> {
    core.yt.radio(&id).await.map_err(e)
}

#[tauri::command]
pub async fn get_lyrics(core: St<'_>, id: String) -> Res<Option<LyricsOut>> {
    core.yt.lyrics(&id).await.map_err(e)
}

#[tauri::command]
pub async fn prefetch_stream(core: St<'_>, id: String) -> Res<()> {
    if core.db.download_path(&id).is_none() {
        core.yt.stream(&id).await.map_err(e)?;
    }
    Ok(())
}

// ------------------------------------------------------------- library
#[tauri::command]
pub fn liked_tracks(core: St) -> Res<Vec<Track>> {
    core.db.liked_tracks().map_err(e)
}

#[tauri::command]
pub fn liked_ids(core: St) -> Res<Vec<String>> {
    core.db.liked_ids().map_err(e)
}

#[tauri::command]
pub fn set_liked(core: St, track: Track, liked: bool) -> Res<()> {
    core.db.set_liked(&track, liked).map_err(e)
}

#[tauri::command]
pub fn list_playlists(core: St) -> Res<Vec<LocalPlaylistSummary>> {
    core.db.list_playlists().map_err(e)
}

#[tauri::command]
pub fn get_local_playlist(core: St, id: i64) -> Res<LocalPlaylist> {
    core.db.get_playlist(id).map_err(e)
}

#[tauri::command]
pub fn create_playlist(core: St, name: String, tracks: Vec<Track>) -> Res<i64> {
    core.db.create_playlist(&name, &tracks).map_err(e)
}

#[tauri::command]
pub fn rename_playlist(core: St, id: i64, name: String) -> Res<()> {
    core.db.rename_playlist(id, &name).map_err(e)
}

#[tauri::command]
pub fn delete_playlist(core: St, id: i64) -> Res<()> {
    core.db.delete_playlist(id).map_err(e)
}

#[tauri::command]
pub fn add_to_playlist(core: St, id: i64, tracks: Vec<Track>) -> Res<()> {
    core.db.add_to_playlist(id, &tracks).map_err(e)
}

#[tauri::command]
pub fn remove_from_playlist(core: St, id: i64, pos: usize) -> Res<()> {
    core.db.remove_from_playlist(id, pos).map_err(e)
}

#[tauri::command]
pub fn record_play(core: St, track: Track) -> Res<()> {
    core.db.record_play(&track).map_err(e)
}

#[tauri::command]
pub fn get_history(core: St) -> Res<Vec<Track>> {
    core.db.history(200).map_err(e)
}

// ------------------------------------------------------------- downloads
#[tauri::command]
pub fn download_tracks(app: AppHandle, core: St, tracks: Vec<Track>) -> Res<()> {
    downloads::enqueue(app, core.inner().clone(), tracks);
    Ok(())
}

#[tauri::command]
pub fn list_downloads(core: St) -> Res<Vec<DownloadEntry>> {
    let mut list = core.db.downloads().map_err(e)?;
    // Drop entries whose file was deleted outside the app.
    list.retain(|d| {
        let ok = std::path::Path::new(&d.path).is_file();
        if !ok {
            let _ = core.db.remove_download(&d.track.id);
        }
        ok
    });
    Ok(list)
}

#[tauri::command]
pub fn downloaded_ids(core: St) -> Res<Vec<String>> {
    core.db.downloaded_ids().map_err(e)
}

#[tauri::command]
pub fn delete_download(core: St, id: String) -> Res<()> {
    if let Some(path) = core.db.remove_download(&id).map_err(e)? {
        let _ = std::fs::remove_file(path);
    }
    Ok(())
}

#[tauri::command]
pub fn open_downloads_dir(core: St) -> Res<()> {
    let dir = core.download_dir();
    std::fs::create_dir_all(&dir).map_err(e)?;
    #[cfg(windows)]
    let opener = "explorer";
    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(all(unix, not(target_os = "macos")))]
    let opener = "xdg-open";
    std::process::Command::new(opener).arg(&dir).spawn().map_err(e)?;
    Ok(())
}

// ------------------------------------------------------------- settings & account
#[tauri::command]
pub async fn get_settings(core: St<'_>) -> Res<Settings> {
    let ytdlp = match ytdlp::find(&core.ytdlp_dir()) {
        Some(bin) => Some(ytdlp::version(&bin).await.map_or_else(
            || bin.display().to_string(),
            |v| format!("{} ({v})", bin.display()),
        )),
        None => None,
    };
    Ok(Settings {
        download_dir: core.download_dir().to_string_lossy().into_owned(),
        logged_in: core.db.get_setting("logged_in").is_some(),
        account_name: core.db.get_setting("account_name"),
        ytdlp,
        ytdlp_supported: cfg!(desktop),
    })
}

#[tauri::command]
pub fn set_download_dir(core: St, path: String) -> Res<()> {
    let p = std::path::PathBuf::from(path.trim());
    if !p.is_absolute() {
        return Err("An absolute path is required, e.g. D:\\Music\\YT Music".into());
    }
    std::fs::create_dir_all(&p).map_err(e)?;
    core.db.set_setting("download_dir", &p.to_string_lossy()).map_err(e)
}

#[tauri::command]
pub async fn login_cookies(core: St<'_>, text: String) -> Res<()> {
    core.yt.login(&text).await.map_err(e)?;
    core.db.set_setting("logged_in", "1").map_err(e)?;
    Ok(())
}

#[tauri::command]
pub async fn logout(core: St<'_>) -> Res<()> {
    core.yt.logout().await.map_err(e)?;
    core.db.del_setting("logged_in").map_err(e)?;
    core.db.del_setting("account_name").map_err(e)?;
    core.db.del_setting("remote_playlists").map_err(e)?;
    Ok(())
}

#[tauri::command]
pub async fn sync_account(core: St<'_>) -> Res<SyncResult> {
    if core.db.get_setting("logged_in").is_none() {
        return Err("Log in first".into());
    }
    let (liked, pls) = tokio::join!(core.yt.liked_tracks(), core.yt.saved_playlists());
    let liked = liked.map_err(|err| format!("liked songs: {err:#}"))?;
    core.db.import_likes(&liked).map_err(e)?;
    let pls = pls.unwrap_or_default();
    let json = serde_json::to_string(&pls).map_err(e)?;
    core.db.set_setting("remote_playlists", &json).map_err(e)?;
    Ok(SyncResult { liked: liked.len(), playlists: pls.len() })
}

#[tauri::command]
pub fn remote_playlists(core: St) -> Res<Vec<PlaylistCard>> {
    Ok(core
        .db
        .get_setting("remote_playlists")
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default())
}

#[tauri::command]
pub async fn install_ytdlp(core: St<'_>) -> Res<String> {
    ytdlp::install(&core.ytdlp_dir(), &core.http)
        .await
        .map(|p| p.display().to_string())
        .map_err(e)
}
