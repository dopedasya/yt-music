//! "Sign in with Google": a separate private window shows Google's own sign-in page;
//! once it lands on music.youtube.com we read the session cookies from the webview and
//! hand them to rustypipe. The user never sees or copies cookies.
//!
//! The window is incognito, so the Google session is not kept in the webview profile:
//! only the YouTube cookies we hand to rustypipe stay, and logging out removes them.
use crate::state::Core;
use std::sync::Arc;
use tauri::webview::PageLoadEvent;
use tauri::{AppHandle, Emitter, Manager, State, Url, WebviewUrl, WebviewWindowBuilder};

const LABEL: &str = "google-login";
const START: &str = "https://accounts.google.com/ServiceLogin?service=youtube&passive=true&continue=https%3A%2F%2Fmusic.youtube.com%2F";
/// A regular desktop browser UA: Google refuses sign-in in "embedded" browsers it can detect.
/// Must stay identical to `additionalBrowserArgs` of the main window in tauri.conf.json.
#[cfg(windows)]
const MAIN_BROWSER_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --enable-features=HardwareMediaKeyHandling --autoplay-policy=no-user-gesture-required";
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36 Edg/142.0.0.0";

#[derive(Clone, serde::Serialize)]
struct LoginResult {
    ok: bool,
    error: Option<String>,
}

fn done(app: &AppHandle, ok: bool, error: Option<String>) {
    let _ = app.emit("google-login", LoginResult { ok, error });
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.close();
    }
}

/// Read YouTube cookies from the login window and log rustypipe in with them.
async fn capture(app: AppHandle, core: Arc<Core>) {
    let Some(w) = app.get_webview_window(LABEL) else { return };
    let url = Url::parse("https://music.youtube.com/").expect("static url");
    // Must not run on the event-handler thread (WebView2 deadlocks); we're in a task.
    let cookies = match tauri::async_runtime::spawn_blocking(move || w.cookies_for_url(url)).await {
        Ok(Ok(c)) => c,
        Ok(Err(e)) => return done(&app, false, Some(format!("{e}"))),
        Err(e) => return done(&app, false, Some(format!("{e}"))),
    };
    // SAPISID is what authenticated YouTube requests are signed with.
    if !cookies.iter().any(|c| c.name() == "SAPISID" || c.name() == "__Secure-3PAPISID") {
        return; // not signed in yet (e.g. landed on music.youtube.com as a guest) — keep waiting
    }
    let header = cookies.iter().map(|c| format!("{}={}", c.name(), c.value())).collect::<Vec<_>>().join("; ");
    match core.yt.login(&header).await {
        Ok(()) => {
            let _ = core.db.set_setting("logged_in", "1");
            done(&app, true, None);
        }
        Err(e) => done(&app, false, Some(format!("{e:#}"))),
    }
}

#[tauri::command]
pub async fn google_login(app: AppHandle, core: State<'_, Arc<Core>>) -> Result<(), String> {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.set_focus();
        return Ok(());
    }
    let core = core.inner().clone();
    let handler_app = app.clone();
    let url = Url::parse(START).map_err(|e| e.to_string())?;
    let builder = WebviewWindowBuilder::new(&app, LABEL, WebviewUrl::External(url));
    // WebView2 shares one browser process per data folder; every window must use the same
    // browser arguments as the main window (tauri.conf.json), or creation fails with
    // 0x8007139F "group or resource is not in the correct state".
    #[cfg(windows)]
    let builder = builder.additional_browser_args(MAIN_BROWSER_ARGS);
    let window = builder
        .title("Sign in with Google")
        // automated tests create it off-screen
        .visible(std::env::var_os("YTUBEM_HIDDEN").is_none())
        .inner_size(480.0, 720.0)
        .resizable(true)
        .incognito(true)
        .user_agent(UA)
        .on_page_load(move |_w, payload| {
            let host = payload.url().host_str().unwrap_or("");
            if payload.event() == PageLoadEvent::Finished && host == "music.youtube.com" {
                tauri::async_runtime::spawn(capture(handler_app.clone(), core.clone()));
            }
        })
        .build()
        .map_err(|e| e.to_string())?;
    // Closing the window without finishing counts as cancel.
    let cancel_app = app.clone();
    window.on_window_event(move |ev| {
        if matches!(ev, tauri::WindowEvent::Destroyed) {
            let _ = cancel_app.emit("google-login-closed", ());
        }
    });
    Ok(())
}
