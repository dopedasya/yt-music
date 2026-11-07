//! Optional yt-dlp fallback: locate/install the binary and ask it for stream URLs or files.
use anyhow::{Result, anyhow, bail};
use std::path::{Path, PathBuf};
use tokio::process::Command;

const EXE: &str = if cfg!(windows) { "yt-dlp.exe" } else { "yt-dlp" };
const RELEASE_URL: &str = if cfg!(windows) {
    "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe"
} else {
    "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp"
};

/// App-local copy first, then PATH.
pub fn find(dir: &Path) -> Option<PathBuf> {
    let local = dir.join(EXE);
    if local.is_file() {
        return Some(local);
    }
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|p| p.join(EXE))
            .find(|p| p.is_file())
    })
}

fn cmd(bin: &Path) -> Command {
    let mut c = Command::new(bin);
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c.kill_on_drop(true);
    c
}

fn watch_url(id: &str) -> String {
    format!("https://music.youtube.com/watch?v={id}")
}

pub async fn get_url(dir: &Path, id: &str) -> Result<String> {
    let bin = find(dir).ok_or_else(|| anyhow!("yt-dlp is not installed"))?;
    let out = cmd(&bin)
        .args(["-f", "bestaudio[ext=m4a]/bestaudio", "--get-url", "--no-playlist", "--no-warnings"])
        .arg(watch_url(id))
        .output()
        .await?;
    if !out.status.success() {
        bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()
        .map(|s| s.trim().to_string())
        .filter(|s| s.starts_with("http"))
        .ok_or_else(|| anyhow!("yt-dlp returned no URL"))
}

/// Download audio straight to `dest` (m4a preferred so it can be tagged).
pub async fn download(dir: &Path, id: &str, dest: &Path) -> Result<()> {
    let bin = find(dir).ok_or_else(|| anyhow!("yt-dlp is not installed"))?;
    let out = cmd(&bin)
        .args(["-f", "bestaudio[ext=m4a]/bestaudio", "--no-playlist", "--no-warnings", "--no-part", "-o"])
        .arg(dest)
        .arg(watch_url(id))
        .output()
        .await?;
    if !out.status.success() {
        bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(())
}

pub async fn install(dir: &Path, http: &reqwest::Client) -> Result<PathBuf> {
    tokio::fs::create_dir_all(dir).await?;
    let bytes = http.get(RELEASE_URL).send().await?.error_for_status()?.bytes().await?;
    let tmp = dir.join(format!("{EXE}.tmp"));
    tokio::fs::write(&tmp, &bytes).await?;
    let dest = dir.join(EXE);
    tokio::fs::rename(&tmp, &dest).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(dest)
}

/// Make sure a usable yt-dlp exists: install the app-local copy if there is none,
/// and self-update it every few days (YouTube changes often, yt-dlp keeps up).
pub async fn ensure(dir: &Path, http: &reqwest::Client) -> Result<()> {
    let local = dir.join(EXE);
    match find(dir) {
        None => {
            install(dir, http).await?;
        }
        Some(bin) if bin == local => {
            let age = std::fs::metadata(&bin)?.modified()?.elapsed().unwrap_or_default();
            if age > std::time::Duration::from_secs(3 * 24 * 3600) {
                let out = cmd(&bin).arg("-U").output().await?;
                if !out.status.success() {
                    // standalone exe can't always self-update (e.g. locked); re-download instead
                    install(dir, http).await?;
                }
                // bump mtime so we don't retry on every start when already up to date
                let f = std::fs::File::options().write(true).open(&bin)?;
                f.set_modified(std::time::SystemTime::now())?;
            }
        }
        Some(_) => {} // system-wide install, managed by the user
    }
    Ok(())
}

pub async fn version(bin: &Path) -> Option<String> {
    let out = cmd(bin).arg("--version").output().await.ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}
