//! Ambil warna dominan dari cover album dan bangun gradient 256 langkah.

use std::collections::HashMap;

pub type Rgb = [u8; 3];

pub fn default_palette() -> Vec<Rgb> {
    vec![[140, 96, 255], [64, 170, 255], [255, 96, 176], [255, 176, 72]]
}

fn rgb_to_hsv(c: Rgb) -> (f32, f32, f32) {
    let (r, g, b) = (c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0);
    let mx = r.max(g).max(b);
    let mn = r.min(g).min(b);
    let d = mx - mn;
    let h = if d == 0.0 {
        0.0
    } else if mx == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if mx == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0, if mx == 0.0 { 0.0 } else { d / mx }, mx)
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> Rgb {
    let h6 = h.rem_euclid(1.0) * 6.0;
    let i = h6.floor();
    let f = h6 - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    let (r, g, b) = match i as i32 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    [
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
    ]
}

fn dist(a: Rgb, b: Rgb) -> f32 {
    let d = |i: usize| a[i] as f32 - b[i] as f32;
    (d(0) * d(0) + d(1) * d(1) + d(2) * d(2)).sqrt()
}

/// Buat warna "menyala": naikkan saturasi & kecerahan (kecuali cover abu-abu).
fn neon(c: Rgb) -> Rgb {
    let (h, s, v) = rgb_to_hsv(c);
    let s2 = if s < 0.12 { s } else { s.max(0.6) };
    hsv_to_rgb(h, s2, v.max(0.88))
}

/// `rgba` = piksel RGBA lurus (thumbnail kecil, mis. 48x48).
pub fn extract(rgba: &[u8]) -> Vec<Rgb> {
    // bucket 5 bit per kanal
    let mut map: HashMap<u16, (u32, u32, u32, u32)> = HashMap::new();
    for p in rgba.chunks_exact(4) {
        if p[3] < 128 {
            continue;
        }
        let key = (((p[0] as u16) >> 3) << 10) | (((p[1] as u16) >> 3) << 5) | ((p[2] as u16) >> 3);
        let e = map.entry(key).or_insert((0, 0, 0, 0));
        e.0 += 1;
        e.1 += p[0] as u32;
        e.2 += p[1] as u32;
        e.3 += p[2] as u32;
    }
    let mut cands: Vec<(f32, Rgb)> = map
        .values()
        .map(|&(n, r, g, b)| {
            let c = [(r / n) as u8, (g / n) as u8, (b / n) as u8];
            let (_, s, v) = rgb_to_hsv(c);
            let score = n as f32 * (0.15 + s) * (0.3 + v.min(0.9));
            (score, c)
        })
        .collect();
    cands.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut out: Vec<Rgb> = Vec::new();
    for (_, c) in cands {
        if out.iter().all(|&o| dist(o, c) > 64.0) {
            out.push(c);
            if out.len() == 4 {
                break;
            }
        }
    }
    if out.is_empty() {
        return default_palette();
    }
    let mut out: Vec<Rgb> = out.into_iter().map(neon).collect();
    if out.len() == 1 {
        let (h, s, v) = rgb_to_hsv(out[0]);
        out.push(hsv_to_rgb(h + 0.08, s, v));
        out.push(hsv_to_rgb(h - 0.08, s, v));
    }
    out
}

/// Gradient siklik 256 langkah dari palet.
pub fn build_lut(pal: &[Rgb]) -> [[u8; 3]; 256] {
    let n = pal.len().max(1);
    let mut lut = [[0u8; 3]; 256];
    for (i, slot) in lut.iter_mut().enumerate() {
        let t = i as f32 / 256.0 * n as f32;
        let a = (t.floor() as usize) % n;
        let b = (a + 1) % n;
        let f = t - t.floor();
        let f = f * f * (3.0 - 2.0 * f);
        let pa = pal.get(a).copied().unwrap_or([255, 255, 255]);
        let pb = pal.get(b).copied().unwrap_or(pa);
        for k in 0..3 {
            slot[k] = (pa[k] as f32 * (1.0 - f) + pb[k] as f32 * f) as u8;
        }
    }
    lut
}
