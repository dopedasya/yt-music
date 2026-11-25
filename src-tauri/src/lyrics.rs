//! Lyrics: time-synced lines from LRCLIB (open lyrics database), falling back to
//! YouTube Music's plain lyrics. All parsing and matching happens here; the UI only
//! highlights the line whose timestamp matches the playback position.
use crate::models::Track;
use anyhow::Result;
use serde::{Deserialize, Serialize};

const LRCLIB: &str = "https://lrclib.net/api";
/// LRCLIB asks clients to identify themselves.
const UA: &str = "YT Music (https://github.com/neverh8me/yt-music)";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LyricLine {
    /// Start time in milliseconds.
    pub t: u64,
    pub text: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Lyrics {
    /// Time-coded lines; empty when only plain text is known.
    pub synced: Vec<LyricLine>,
    pub plain: Option<String>,
    pub source: String,
    pub instrumental: bool,
}

#[derive(Deserialize)]
struct LrclibEntry {
    #[serde(rename = "trackName", default)]
    track_name: String,
    #[serde(rename = "artistName", default)]
    artist_name: String,
    #[serde(default)]
    duration: f64,
    #[serde(default)]
    instrumental: bool,
    #[serde(rename = "plainLyrics")]
    plain_lyrics: Option<String>,
    #[serde(rename = "syncedLyrics")]
    synced_lyrics: Option<String>,
}

/// Parse LRC (`[mm:ss.xx] text`, several tags per line allowed). Metadata tags such
/// as `[ar:...]` are skipped. Result is sorted by time.
pub fn parse_lrc(lrc: &str) -> Vec<LyricLine> {
    let mut out = Vec::new();
    for raw in lrc.lines() {
        let mut rest = raw.trim();
        let mut stamps = Vec::new();
        while let Some(body) = rest.strip_prefix('[') {
            let Some(end) = body.find(']') else { break };
            match parse_stamp(&body[..end]) {
                Some(ms) => stamps.push(ms),
                None => break, // metadata tag like [ar:Artist]
            }
            rest = &body[end + 1..];
        }
        let text = rest.trim().to_string();
        for t in stamps {
            out.push(LyricLine { t, text: text.clone() });
        }
    }
    out.sort_by_key(|l| l.t);
    out
}

fn parse_stamp(s: &str) -> Option<u64> {
    let (m, sec) = s.split_once(':')?;
    let m: u64 = m.trim().parse().ok()?;
    let sec: f64 = sec.trim().replace(',', ".").parse().ok()?;
    if !(0.0..60.0).contains(&sec) {
        return None;
    }
    Some(m * 60_000 + (sec * 1000.0).round() as u64)
}

/// Normalise for fuzzy comparison: lowercase, drop "(feat. ...)"/"[...]" parts and punctuation.
fn norm(s: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in s.to_lowercase().chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            _ if depth > 0 => {}
            c if c.is_alphanumeric() => out.push(c),
            _ => out.push(' '),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Score a search hit: title must match, artist and duration break ties. Lower is better.
fn score(e: &LrclibEntry, title: &str, artist: &str, duration: Option<u32>) -> Option<f64> {
    let (et, ea) = (norm(&e.track_name), norm(&e.artist_name));
    let (t, a) = (norm(title), norm(artist));
    if et.is_empty() || !(et == t || et.contains(&t) || t.contains(&et)) {
        return None;
    }
    let mut s = if et == t { 0.0 } else { 5.0 };
    if !a.is_empty() && !(ea.contains(&a) || a.contains(&ea)) {
        s += 20.0;
    }
    if let Some(d) = duration {
        let diff = (e.duration - d as f64).abs();
        if diff > 15.0 {
            return None; // probably a different version (live, remix, extended)
        }
        s += diff;
    }
    if e.synced_lyrics.is_none() {
        s += 3.0;
    }
    Some(s)
}

fn into_lyrics(e: LrclibEntry) -> Lyrics {
    Lyrics {
        synced: e.synced_lyrics.as_deref().map(parse_lrc).unwrap_or_default(),
        plain: e.plain_lyrics.filter(|p| !p.trim().is_empty()),
        source: "LRCLIB".into(),
        instrumental: e.instrumental,
    }
}

pub async fn lrclib(http: &reqwest::Client, t: &Track) -> Result<Option<Lyrics>> {
    let artist = t.artists.first().map(|a| a.name.as_str()).unwrap_or("");
    // 1) exact lookup (fast, uses LRCLIB's own matching incl. duration)
    let mut q = vec![("track_name", t.title.as_str()), ("artist_name", artist)];
    if let Some(a) = &t.album {
        q.push(("album_name", a.name.as_str()));
    }
    let dur = t.duration.map(|d| d.to_string());
    if let Some(d) = &dur {
        q.push(("duration", d.as_str()));
    }
    let r = http.get(format!("{LRCLIB}/get")).query(&q).header("User-Agent", UA).send().await?;
    if r.status().is_success() {
        let e: LrclibEntry = r.json().await?;
        if e.synced_lyrics.is_some() || e.plain_lyrics.is_some() || e.instrumental {
            return Ok(Some(into_lyrics(e)));
        }
    }
    // 2) search and pick the closest match
    let hits: Vec<LrclibEntry> = http
        .get(format!("{LRCLIB}/search"))
        .query(&[("track_name", t.title.as_str()), ("artist_name", artist)])
        .header("User-Agent", UA)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let best = hits
        .into_iter()
        .filter_map(|e| score(&e, &t.title, artist, t.duration).map(|s| (s, e)))
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, e)| into_lyrics(e));
    Ok(best)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lrc_parsing() {
        let l = parse_lrc("[ar:Someone]\n[00:12.50]Hello\n[00:05.00][01:00.25]Chorus\n[00:20.00]\n");
        assert_eq!(
            l,
            vec![
                LyricLine { t: 5000, text: "Chorus".into() },
                LyricLine { t: 12500, text: "Hello".into() },
                LyricLine { t: 20000, text: "".into() },
                LyricLine { t: 60250, text: "Chorus".into() },
            ]
        );
    }

    #[test]
    fn normalisation() {
        assert_eq!(norm("Обійми (feat. X) [Live]"), "обійми");
        assert_eq!(norm("Don't Stop Me Now!"), "don t stop me now");
    }
}
