# YT Music

**English** · [Українська](README.uk.md) · [Русский](README.ru.md)

A fast, unofficial YouTube Music desktop client written in Rust (Tauri 2). Search, stream, build a library, and download music for offline listening, with an interface inspired by the big streaming apps.

> Unofficial project. Not affiliated with, endorsed by, or sponsored by Google LLC. YouTube and YouTube Music are trademarks of Google LLC.

## Features

- Search songs, albums, artists and playlists, with suggestions as you type
- Album, artist and playlist pages
- Player with queue, shuffle, repeat (all / one), seeking, volume and media keys
- Autoplay: when the queue runs out, similar songs (radio) are added
- Lyrics
- Library: liked songs, your own playlists and history, stored locally in SQLite
- Offline downloads as `.m4a` with tags and cover art; downloaded songs play without internet
- YouTube Music account (via cookies): import your liked songs and playlists
- Interface in English, Ukrainian and Russian
- Small and fast: ~14 MB binary, a song starts in ~0.2 s

## Install

Grab the installer for your platform from [Releases](../../releases).

- **Windows**: `YT Music_x.y.z_x64-setup.exe`
- **macOS**: `.dmg` (Apple Silicon and Intel). The app isn't notarized, so on first launch right-click it and choose **Open**.
- **Linux**: `.AppImage` / `.deb`
- **Android**: coming soon

## Build from source

Requirements: [Rust](https://rustup.rs) (stable). On Windows you also need the MSVC Build Tools and WebView2 (preinstalled on Windows 10/11). On Linux you need the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).

```sh
cd src-tauri
cargo run                          # dev build
cargo install tauri-cli --version "^2"
cargo tauri build                  # installer in target/release/bundle/
```

The frontend is plain HTML/CSS/JS in `ui/`, so no Node.js or bundler is needed.

## How it works

```
ui/                 frontend (no build step): app.js, player.js, api.js, i18n.js, icons.js
src-tauri/src/
  main.rs           startup, command registration
  commands.rs       Tauri commands called by the UI
  yt.rs             YouTube Music metadata (rustypipe) + audio stream resolution
  server.rs         local 127.0.0.1 HTTP proxy for <audio> (Range requests / seeking)
  downloads.rs      chunked downloads + tagging (lofty)
  db.rs             SQLite: likes, playlists, history, downloads, settings
  ytdlp.rs          optional yt-dlp fallback (desktop only)
```

- **Metadata** (search, albums, artists, radio, lyrics, account) comes from [rustypipe](https://codeberg.org/ThetaDev/rustypipe).
- **Audio URLs** come from a direct InnerTube `player` request with the visionOS client, which needs no JS player and no PO token. This resolver races rustypipe, and the first verified URL wins. On desktop, [yt-dlp](https://github.com/yt-dlp/yt-dlp) is a last-resort fallback; it is installed on first run and updated automatically.
- googlevideo rejects open-ended ranges, so the local proxy fetches upstream in bounded 4 MB chunks and stitches them into one stream for the player. Seeking works, and stream URLs never reach the web layer.
- The next song in the queue is resolved in advance, so skipping is instant.

## Tests

```sh
cargo test                            # offline tests
cargo test -- --ignored --nocapture   # live tests against real YouTube
```

## Keyboard shortcuts

| Key | Action |
|---|---|
| Space | Play / pause |
| ← / → | Seek −5 / +5 s |
| Ctrl+← / Ctrl+→ | Previous / next |
| Ctrl+↑ / Ctrl+↓ | Volume |
| Ctrl+K or / | Search |
| Alt+L | Like current song |
| Alt+← | Back |

## License

[GPL-3.0](LICENSE). The project depends on rustypipe, which is GPL-3.0, so the app as a whole is distributed under the same license.
