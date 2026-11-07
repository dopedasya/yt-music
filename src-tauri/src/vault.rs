//! Offline storage in our own container, like Spotify's / YouTube Music's offline cache:
//! the original YouTube audio stream (AAC or Opus, untouched) encrypted with ChaCha20.
//! Files only play inside the app; to any other player they are noise.
//!
//! Layout of a `.ytmc` file (all header fields plaintext):
//!   magic  "YTMC" | version u8 | nonce [12] | mime_len u8 | mime | payload (encrypted)
//!
//! ChaCha20 is a seekable stream cipher, so any byte range can be decrypted on its own —
//! that's what makes seeking in offline tracks work.
//!
//! The key is generated per install and stored in the app database. This keeps files
//! from being copied around as plain music; it is not meant to resist someone who
//! extracts the key from the same machine.
use anyhow::{Result, anyhow, bail};
use chacha20::ChaCha20;
use chacha20::cipher::{KeyIvInit, StreamCipher, StreamCipherSeek};
use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

const MAGIC: &[u8; 4] = b"YTMC";
const VERSION: u8 = 1;
pub const EXT: &str = "ytmc";

#[derive(Clone)]
pub struct Key([u8; 32]);

impl Key {
    pub fn from_raw(k: [u8; 32]) -> Self {
        Key(k)
    }

    #[cfg(test)]
    pub fn generate() -> Result<Self> {
        let mut k = [0u8; 32];
        getrandom::fill(&mut k).map_err(|e| anyhow!("rng: {e}"))?;
        Ok(Key(k))
    }

    #[cfg(test)]
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[cfg(test)]
    pub fn from_hex(s: &str) -> Result<Self> {
        if s.len() != 64 {
            bail!("bad key length");
        }
        let mut k = [0u8; 32];
        for (i, b) in k.iter_mut().enumerate() {
            *b = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)?;
        }
        Ok(Key(k))
    }
}

/// Parsed container header.
pub struct Header {
    pub nonce: [u8; 12],
    pub mime: String,
    /// Offset of the encrypted payload in the file.
    pub data_start: u64,
}

fn cipher(key: &Key, nonce: &[u8; 12], offset: u64) -> ChaCha20 {
    let mut c = ChaCha20::new(&key.0.into(), nonce.into());
    c.seek(offset);
    c
}

/// Streaming writer: header first, then encrypted chunks as they arrive.
pub struct Writer {
    file: tokio::fs::File,
    cipher: ChaCha20,
}

impl Writer {
    pub async fn create(path: &Path, key: &Key, mime: &str) -> Result<Self> {
        let mut nonce = [0u8; 12];
        getrandom::fill(&mut nonce).map_err(|e| anyhow!("rng: {e}"))?;
        let mime = mime.as_bytes();
        let mut header = Vec::with_capacity(18 + mime.len());
        header.extend_from_slice(MAGIC);
        header.push(VERSION);
        header.extend_from_slice(&nonce);
        header.push(u8::try_from(mime.len())?);
        header.extend_from_slice(mime);
        let mut file = tokio::fs::File::create(path).await?;
        file.write_all(&header).await?;
        Ok(Writer { file, cipher: cipher(key, &nonce, 0) })
    }

    pub async fn write(&mut self, plain: &[u8]) -> Result<()> {
        let mut buf = plain.to_vec();
        self.cipher.apply_keystream(&mut buf);
        self.file.write_all(&buf).await?;
        Ok(())
    }

    pub async fn finish(mut self) -> Result<()> {
        self.file.flush().await?;
        self.file.sync_all().await?;
        Ok(())
    }
}

pub async fn read_header(file: &mut tokio::fs::File) -> Result<Header> {
    let mut fixed = [0u8; 18];
    file.seek(std::io::SeekFrom::Start(0)).await?;
    file.read_exact(&mut fixed).await?;
    if &fixed[..4] != MAGIC {
        bail!("not a .ytmc file");
    }
    if fixed[4] != VERSION {
        bail!("unsupported .ytmc version {}", fixed[4]);
    }
    let nonce: [u8; 12] = fixed[5..17].try_into()?;
    let mime_len = fixed[17] as usize;
    let mut mime = vec![0u8; mime_len];
    file.read_exact(&mut mime).await?;
    Ok(Header { nonce, mime: String::from_utf8(mime)?, data_start: 18 + mime_len as u64 })
}

/// Decrypting reader for an arbitrary byte range of the payload.
pub struct RangeReader {
    file: tokio::fs::File,
    cipher: ChaCha20,
    remaining: u64,
}

impl RangeReader {
    pub async fn open(mut file: tokio::fs::File, header: &Header, key: &Key, start: u64, len: u64) -> Result<Self> {
        file.seek(std::io::SeekFrom::Start(header.data_start + start)).await?;
        Ok(RangeReader { file, cipher: cipher(key, &header.nonce, start), remaining: len })
    }

    /// Next decrypted chunk, `None` at the end of the range.
    pub async fn next_chunk(&mut self) -> Result<Option<Vec<u8>>> {
        if self.remaining == 0 {
            return Ok(None);
        }
        let n = self.remaining.min(256 * 1024) as usize;
        let mut buf = vec![0u8; n];
        let got = self.file.read(&mut buf).await?;
        if got == 0 {
            return Ok(None);
        }
        buf.truncate(got);
        self.cipher.apply_keystream(&mut buf);
        self.remaining -= got as u64;
        Ok(Some(buf))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn roundtrip_and_random_access() {
        let dir = std::env::temp_dir().join("ytubem-vault-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.ytmc");
        let key = Key::generate().unwrap();
        let data: Vec<u8> = (0..1_000_000u32).map(|i| (i * 7 % 251) as u8).collect();

        let mut w = Writer::create(&path, &key, "audio/mp4").await.unwrap();
        for chunk in data.chunks(77_777) {
            w.write(chunk).await.unwrap();
        }
        w.finish().await.unwrap();

        let raw = std::fs::read(&path).unwrap();
        assert_ne!(&raw[raw.len() - 1000..], &data[data.len() - 1000..], "payload is encrypted");

        let mut f = tokio::fs::File::open(&path).await.unwrap();
        let h = read_header(&mut f).await.unwrap();
        assert_eq!(h.mime, "audio/mp4");
        for (start, len) in [(0u64, 1000u64), (123_457, 50_000), (999_000, 1000)] {
            let f = tokio::fs::File::open(&path).await.unwrap();
            let mut r = RangeReader::open(f, &h, &key, start, len).await.unwrap();
            let mut out = Vec::new();
            while let Some(c) = r.next_chunk().await.unwrap() {
                out.extend(c);
            }
            assert_eq!(out, data[start as usize..(start + len) as usize]);
        }
        let other = Key::generate().unwrap();
        let f = tokio::fs::File::open(&path).await.unwrap();
        let mut r = RangeReader::open(f, &h, &other, 0, 1000).await.unwrap();
        assert_ne!(r.next_chunk().await.unwrap().unwrap(), data[..1000], "wrong key gives noise");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn key_hex() {
        let k = Key::generate().unwrap();
        assert_eq!(Key::from_hex(&k.to_hex()).unwrap().0, k.0);
    }
}
