//! Lirik tersinkron dari LRCLIB (https://lrclib.net, gratis, tanpa akun).
//! HTTP lewat curl.exe bawaan Windows 10 (1803+) / 11 -> nol dependensi jaringan.

use std::os::windows::process::CommandExt;

pub struct Lyrics {
    pub lines: Vec<(f64, String)>,
}

impl Lyrics {
    /// Indeks baris yang sedang dinyanyikan pada detik `t`.
    pub fn index_at(&self, t: f64) -> Option<usize> {
        let n = self.lines.partition_point(|l| l.0 <= t);
        if n == 0 {
            None
        } else {
            Some(n - 1)
        }
    }
}

fn curl(args: &[&str]) -> Option<Vec<u8>> {
    let out = std::process::Command::new("curl.exe")
        .args(["-s", "-m", "8", "-A", "LumaBeat/0.1 (desktop lyrics)", "-G"])
        .args(args)
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .output()
        .ok()?;
    if out.status.success() && !out.stdout.is_empty() {
        Some(out.stdout)
    } else {
        None
    }
}

fn clean_title(t: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for ch in t.chars() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => {
                if depth > 0 {
                    depth -= 1;
                }
            }
            _ => {
                if depth == 0 {
                    out.push(ch);
                }
            }
        }
    }
    out.trim().to_string()
}

fn clean_artist(a: &str) -> String {
    a.trim().trim_end_matches(" - Topic").trim().to_string()
}

pub fn fetch(title: &str, artist: &str, dur: f64) -> Option<Lyrics> {
    let t = clean_title(title);
    let t = if t.is_empty() { title.trim().to_string() } else { t };
    let a = clean_artist(artist);

    let mut tries: Vec<Vec<String>> = Vec::new();
    if !a.is_empty() {
        tries.push(vec![format!("track_name={}", t), format!("artist_name={}", a)]);
    }
    tries.push(vec![format!("q={} {}", a, t).trim().to_string()]);
    let mut q_only = String::from("q=");
    q_only.push_str(&t);
    tries.push(vec![q_only]);

    for p in &tries {
        let mut args: Vec<&str> = vec!["https://lrclib.net/api/search"];
        for kv in p {
            args.push("--data-urlencode");
            args.push(kv.as_str());
        }
        if let Some(body) = curl(&args) {
            if let Some(l) = pick(&body, dur) {
                return Some(l);
            }
        }
    }
    None
}

fn pick(body: &[u8], dur: f64) -> Option<Lyrics> {
    let v: serde_json::Value = serde_json::from_slice(body).ok()?;
    let arr = v.as_array()?;
    let mut best: Option<(f64, &str)> = None;
    for it in arr {
        let Some(s) = it.get("syncedLyrics").and_then(|x| x.as_str()) else { continue };
        if s.trim().is_empty() {
            continue;
        }
        let d = it.get("duration").and_then(|x| x.as_f64()).unwrap_or(0.0);
        let known = dur > 1.0 && d > 1.0;
        let diff = if known { (d - dur).abs() } else { 0.0 };
        if known && diff > 8.0 {
            continue;
        }
        if best.map_or(true, |b| diff < b.0) {
            best = Some((diff, s));
        }
    }
    let (_, s) = best?;
    let lines = parse_lrc(s);
    if lines.len() < 2 {
        None
    } else {
        Some(Lyrics { lines })
    }
}

fn parse_ts(tag: &str) -> Option<f64> {
    let (m, s) = tag.split_once(':')?;
    let m: f64 = m.trim().parse().ok()?;
    let s: f64 = s.trim().parse().ok()?;
    Some(m * 60.0 + s)
}

pub fn parse_lrc(s: &str) -> Vec<(f64, String)> {
    let mut out: Vec<(f64, String)> = Vec::new();
    for line in s.lines() {
        let mut rest = line.trim();
        let mut times: Vec<f64> = Vec::new();
        while rest.starts_with('[') {
            let Some(end) = rest.find(']') else { break };
            if let Some(t) = parse_ts(&rest[1..end]) {
                times.push(t);
            }
            rest = rest[end + 1..].trim_start();
        }
        for t in times {
            out.push((t, rest.to_string()));
        }
    }
    out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    out
}
