//! Desktop integrations: Discord Rich Presence ("Listening to YT Music"), a system
//! notification when the track changes while the app is in the background, and global
//! media hotkeys. Each can be switched off in Settings.
use crate::models::Track;
use crate::state::Core;
use tauri::{AppHandle, Manager};

pub fn enabled(core: &Core, key: &str) -> bool {
    core.db.get_setting(key).as_deref() != Some("0")
}

// ------------------------------------------------------------------ notifications
/// Notify about the new track, only when the user isn't looking at the app.
pub fn notify_track(app: &AppHandle, core: &Core, t: &Track) {
    if !enabled(core, "notify_tracks") {
        return;
    }
    let focused = app.get_webview_window("main").and_then(|w| w.is_focused().ok()).unwrap_or(false);
    if focused {
        return;
    }
    use tauri_plugin_notification::NotificationExt;
    let artists = t.artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ");
    let _ = app.notification().builder().title(&t.title).body(artists).show();
}

// ------------------------------------------------------------------ Discord
/// Discord application id of "YT Music" (public; from the Discord Developer Portal).
/// Forks can override it at build time with YTUBEM_DISCORD_CLIENT_ID.
#[cfg(desktop)]
pub const DISCORD_CLIENT_ID: &str = env_or(option_env!("YTUBEM_DISCORD_CLIENT_ID"), "1568772055766405213");

#[cfg(desktop)]
const fn env_or(v: Option<&'static str>, default: &'static str) -> &'static str {
    match v {
        Some(s) => s,
        None => default,
    }
}

#[derive(Debug)]
pub enum Presence {
    Playing { track: Track, position_s: f64 },
    Paused,
    Off,
}

/// Owns the (blocking, non-Send-error) Discord IPC client on its own thread.
pub struct Discord {
    #[cfg(desktop)]
    tx: std::sync::mpsc::Sender<Presence>,
}

impl Discord {
    pub fn start() -> Self {
        #[cfg(desktop)]
        {
            let (tx, rx) = std::sync::mpsc::channel::<Presence>();
            std::thread::Builder::new()
                .name("discord-rpc".into())
                .spawn(move || discord_loop(rx))
                .ok();
            Discord { tx }
        }
        #[cfg(mobile)]
        Discord {}
    }

    pub fn update(&self, p: Presence) {
        #[cfg(desktop)]
        {
            let _ = self.tx.send(p);
        }
        #[cfg(mobile)]
        let _ = p;
    }
}

#[cfg(desktop)]
fn discord_loop(rx: std::sync::mpsc::Receiver<Presence>) {
    use discord_rich_presence::activity::{Activity, ActivityType, Assets, Button, Timestamps};
    use discord_rich_presence::{DiscordIpc, DiscordIpcClient};
    if DISCORD_CLIENT_ID.is_empty() {
        while rx.recv().is_ok() {} // drain; feature not configured in this build
        return;
    }
    let mut client: Option<DiscordIpcClient> = None;
    while let Ok(mut msg) = rx.recv() {
        // only the latest state matters
        while let Ok(newer) = rx.try_recv() {
            msg = newer;
        }
        if client.is_none() {
            // Discord may not be running; try again on the next update.
            client = DiscordIpcClient::new(DISCORD_CLIENT_ID).ok().and_then(|mut c| c.connect().ok().map(|_| c));
        }
        let Some(c) = client.as_mut() else { continue };
        let ok = match &msg {
            Presence::Playing { track, position_s } => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                let start = now - *position_s as i64;
                let artists = track.artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ");
                let state = format!("by {artists}");
                let url = format!("https://music.youtube.com/watch?v={}", track.id);
                let album = track.album.as_ref().map(|a| a.name.clone()).unwrap_or_else(|| track.title.clone());
                let mut ts = Timestamps::new().start(start);
                if let Some(d) = track.duration {
                    ts = ts.end(start + d as i64);
                }
                let mut assets = Assets::new().large_text(&album);
                if let Some(cover) = track.cover.as_deref().filter(|c| c.starts_with("https://")) {
                    assets = assets.large_image(cover);
                }
                let activity = Activity::new()
                    .activity_type(ActivityType::Listening)
                    .details(&track.title)
                    .state(&state)
                    .timestamps(ts)
                    .assets(assets)
                    .buttons(vec![Button::new("Open on YouTube Music", &url)]);
                c.set_activity(activity).is_ok()
            }
            Presence::Paused | Presence::Off => c.clear_activity().is_ok(),
        };
        if !ok {
            let _ = c.close();
            client = None; // Discord restarted or closed: reconnect next time
        }
    }
}

// ------------------------------------------------------------------ global hotkeys
/// Ctrl+Alt+Space play/pause · Ctrl+Alt+→ next · Ctrl+Alt+← previous · Ctrl+Alt+L like.
/// The UI performs the action (it owns <audio>); we just forward the key.
#[cfg(desktop)]
pub fn hotkeys_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    use tauri::Emitter;
    use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut, ShortcutState};
    let mods = Some(Modifiers::CONTROL | Modifiers::ALT);
    let map = [
        (Shortcut::new(mods, Code::Space), "toggle"),
        (Shortcut::new(mods, Code::ArrowRight), "next"),
        (Shortcut::new(mods, Code::ArrowLeft), "prev"),
        (Shortcut::new(mods, Code::KeyL), "like"),
    ];
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(move |app, shortcut, event| {
            if event.state() != ShortcutState::Pressed {
                return;
            }
            if let Some(core) = app.try_state::<std::sync::Arc<Core>>()
                && !enabled(&core, "global_hotkeys")
            {
                return;
            }
            if let Some((_, action)) = map.iter().find(|(s, _)| s == shortcut) {
                let _ = app.emit("hotkey", *action);
            }
        })
        .build()
}

#[cfg(desktop)]
pub fn register_hotkeys(app: &AppHandle) {
    use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};
    let mods = Some(Modifiers::CONTROL | Modifiers::ALT);
    for code in [Code::Space, Code::ArrowRight, Code::ArrowLeft, Code::KeyL] {
        // Another app may already own a combination; that one just won't work.
        if let Err(e) = app.global_shortcut().register(Shortcut::new(mods, code)) {
            eprintln!("hotkey {code:?} unavailable: {e}");
        }
    }
}

#[cfg(all(test, desktop))]
mod live_tests {
    /// Connects to the local Discord app and clears the activity (nothing visible to friends).
    #[test]
    #[ignore]
    fn live_discord_handshake() {
        use discord_rich_presence::{DiscordIpc, DiscordIpcClient};
        let mut c = DiscordIpcClient::new(super::DISCORD_CLIENT_ID).expect("client");
        match c.connect() {
            Ok(()) => {
                c.clear_activity().expect("clear");
                c.close().ok();
                println!("discord: connected and cleared with app id {}", super::DISCORD_CLIENT_ID);
            }
            Err(e) => println!("discord not reachable (is the Discord app running?): {e}"),
        }
    }
}
