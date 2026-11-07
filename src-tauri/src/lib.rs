
mod commands;
mod db;
mod downloads;
mod models;
mod server;
mod state;
mod yt;
mod ytdlp;

use state::Core;
use std::sync::Arc;
use std::sync::atomic::{AtomicU16, Ordering};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let default_download_dir = app
                .path()
                .audio_dir()
                .unwrap_or_else(|_| data_dir.join("downloads"))
                .join("YT Music");

            let db = db::Db::open(&data_dir.join("library.db"))?;
            let yt = yt::Yt::new(data_dir.join("rustypipe"), data_dir.join("bin"))?;
            let http = reqwest::Client::builder()
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36")
                .connect_timeout(std::time::Duration::from_secs(10))
                .build()?;

            let core = Arc::new(Core {
                db,
                yt,
                http,
                data_dir,
                default_download_dir,
                port: AtomicU16::new(0),
                download_slots: tokio::sync::Semaphore::new(3),
                downloading: Default::default(),
            });
            let port = tauri::async_runtime::block_on(server::start(core.clone()))?;
            core.port.store(port, Ordering::Relaxed);

            // yt-dlp is a desktop-only fallback for stream resolution.
            #[cfg(desktop)]
            {
                let bg = core.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = ytdlp::ensure(&bg.ytdlp_dir(), &bg.http).await {
                        eprintln!("yt-dlp setup failed: {e:#}");
                    }
                });
            }
            app.manage(core);

            // The window starts hidden (no white flash); show it once set up.
            // YTUBEM_HIDDEN keeps it hidden for automated UI tests.
            if std::env::var_os("YTUBEM_HIDDEN").is_none()
                && let Some(w) = app.get_webview_window("main")
            {
                w.show()?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::stream_base,
            commands::search,
            commands::search_suggestions,
            commands::home,
            commands::get_album,
            commands::get_artist,
            commands::get_playlist,
            commands::get_radio,
            commands::get_lyrics,
            commands::prefetch_stream,
            commands::liked_tracks,
            commands::liked_ids,
            commands::set_liked,
            commands::list_playlists,
            commands::get_local_playlist,
            commands::create_playlist,
            commands::rename_playlist,
            commands::delete_playlist,
            commands::add_to_playlist,
            commands::remove_from_playlist,
            commands::record_play,
            commands::get_history,
            commands::download_tracks,
            commands::list_downloads,
            commands::downloaded_ids,
            commands::delete_download,
            commands::open_downloads_dir,
            commands::get_settings,
            commands::set_download_dir,
            commands::login_cookies,
            commands::logout,
            commands::sync_account,
            commands::remote_playlists,
            commands::install_ytdlp,
        ])
        .run(tauri::generate_context!())
        .expect("error while running YTubeM");
}
