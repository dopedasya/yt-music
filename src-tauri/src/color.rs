//! Dominant cover colour for Spotify-style page gradients.
//! Picks the most "characteristic" colour (frequent *and* saturated), then tunes its
//! lightness so white text stays readable on top of it in the dark theme.
use anyhow::Result;
use image::imageops::FilterType;

/// RGB → HSL, all components in 0..1.
fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if max == min {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } / 6.0;
    (h, s, l)
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let hue = |p: f32, q: f32, mut t: f32| {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    let (r, g, b) = if s == 0.0 {
        (l, l, l)
    } else {
        let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
        let p = 2.0 * l - q;
        (hue(p, q, h + 1.0 / 3.0), hue(p, q, h), hue(p, q, h - 1.0 / 3.0))
    };
    ((r * 255.0).round() as u8, (g * 255.0).round() as u8, (b * 255.0).round() as u8)
}

/// Dominant colour of an encoded image as `#rrggbb`.
pub fn dominant(bytes: &[u8]) -> Result<String> {
    let img = image::load_from_memory(bytes)?.resize_exact(48, 48, FilterType::Triangle).to_rgb8();

    // 24 hue buckets; each pixel votes with weight ∝ saturation, so grey/black/white
    // backgrounds don't win over a smaller but vivid subject.
    const BUCKETS: usize = 24;
    let mut weight = [0f32; BUCKETS];
    let mut sum = [(0f32, 0f32, 0f32); BUCKETS];
    let (mut grey_l, mut n) = (0f32, 0f32);
    for p in img.pixels() {
        let (r, g, b) = (p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0);
        let (h, s, l) = rgb_to_hsl(r, g, b);
        grey_l += l;
        n += 1.0;
        if s < 0.18 || !(0.08..=0.92).contains(&l) {
            continue;
        }
        let i = ((h * BUCKETS as f32) as usize).min(BUCKETS - 1);
        let w = s * (1.0 - (l - 0.5).abs());
        weight[i] += w;
        sum[i].0 += r * w;
        sum[i].1 += g * w;
        sum[i].2 += b * w;
    }

    let (best, &w) = weight
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .unwrap_or((0, &0.0));
    // Mostly grey covers: use a neutral slate tinted by overall brightness.
    if w < n * 0.03 {
        let l = (grey_l / n.max(1.0)).clamp(0.18, 0.32);
        let (r, g, b) = hsl_to_rgb(0.66, 0.08, l);
        return Ok(format!("#{r:02x}{g:02x}{b:02x}"));
    }
    let (r, g, b) = (sum[best].0 / w, sum[best].1 / w, sum[best].2 / w);
    let (h, s, l) = rgb_to_hsl(r, g, b);
    // Keep it rich but dark enough for white text.
    let (r, g, b) = hsl_to_rgb(h, s.clamp(0.35, 0.75), l.clamp(0.28, 0.42));
    Ok(format!("#{r:02x}{g:02x}{b:02x}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 3]) -> Vec<u8> {
        let img = image::RgbImage::from_fn(w, h, |x, y| image::Rgb(f(x, y)));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn vivid_subject_beats_dark_background() {
        // black background with a red square in the middle
        let bytes = png(64, 64, |x, y| if (20..44).contains(&x) && (20..44).contains(&y) { [220, 30, 40] } else { [5, 5, 5] });
        let c = dominant(&bytes).unwrap();
        let r = u8::from_str_radix(&c[1..3], 16).unwrap();
        let g = u8::from_str_radix(&c[3..5], 16).unwrap();
        assert!(r > g * 2, "expected a red tone, got {c}");
    }

    #[test]
    fn grey_cover_gives_neutral() {
        let bytes = png(16, 16, |_, _| [128, 128, 128]);
        assert!(dominant(&bytes).unwrap().starts_with('#'));
    }
}
