//! Thread media.
//! - Mengikuti lagu yang sedang diputar di aplikasi apa pun yang mendukung
//!   System Media Transport Controls (Spotify, Apple Music, browser: YouTube
//!   Music / SoundCloud, dll).
//! - Memutar file/folder lokal lewat MediaPlayer bawaan Windows (codec OS), yang
//!   otomatis muncul di SMTC sehingga cahaya/lirik/player ikut bekerja.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use windows::core::HSTRING;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionMediaProperties as Props,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Media::Core::MediaSource;
use windows::Media::Playback::{MediaPlaybackItem, MediaPlaybackList, MediaPlayer};
use windows::Storage::Streams::DataReader;
use windows::Storage::StorageFile;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
use windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW;

pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Clone)]
pub struct Now {
    pub has_session: bool,
    pub playing: bool,
    pub title: String,
    pub artist: String,
    pub thumb: Option<Arc<Vec<u8>>>,
    pub track_id: u64,
    pub art_ver: u64,
    pub pos: f64,
    pub dur: f64,
    pub stamp: Instant,
}

impl Now {
    pub fn new() -> Now {
        Now {
            has_session: false,
            playing: false,
            title: String::new(),
            artist: String::new(),
            thumb: None,
            track_id: 0,
            art_ver: 0,
            pos: 0.0,
            dur: 0.0,
            stamp: Instant::now(),
        }
    }

    /// Posisi lagu sekarang (detik), diekstrapolasi sejak polling terakhir.
    pub fn position_now(&self) -> f64 {
        let mut p = self.pos;
        if self.playing {
            p += self.stamp.elapsed().as_secs_f64();
        }
        if self.dur > 0.0 && p > self.dur {
            p = self.dur;
        }
        p.max(0.0)
    }
}

pub enum Cmd {
    PlayPause,
    Next,
    Prev,
    Seek(f64),
    Open(Vec<PathBuf>),
}

pub fn spawn(hwnd: isize, shared: Arc<Mutex<Now>>) -> Sender<Cmd> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = worker(hwnd, shared, rx);
    });
    tx
}

struct Worker {
    last_key: String,
    tries: u32,
}

fn notify(hwnd: isize) {
    unsafe {
        PostMessageW(hwnd, crate::WM_APP_MEDIA, 0, 0);
    }
}

fn worker(hwnd: isize, shared: Arc<Mutex<Now>>, rx: Receiver<Cmd>) -> windows::core::Result<()> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let mgr = Manager::RequestAsync()?.get()?;
    let mut player: Option<MediaPlayer> = None;
    let mut w = Worker { last_key: String::new(), tries: 0 };

    loop {
        let wait = if lock(&shared).playing { 400 } else { 1500 };
        match rx.recv_timeout(Duration::from_millis(wait)) {
            Ok(cmd) => handle(cmd, &mgr, &mut player),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        poll(&mgr, &shared, hwnd, &mut w);
    }
    Ok(())
}

fn handle(cmd: Cmd, mgr: &Manager, player: &mut Option<MediaPlayer>) {
    match cmd {
        Cmd::PlayPause => {
            if let Ok(s) = mgr.GetCurrentSession() {
                let _ = s.TryTogglePlayPauseAsync().and_then(|a| a.get());
            }
        }
        Cmd::Next => {
            if let Ok(s) = mgr.GetCurrentSession() {
                let _ = s.TrySkipNextAsync().and_then(|a| a.get());
            }
        }
        Cmd::Prev => {
            if let Ok(s) = mgr.GetCurrentSession() {
                let _ = s.TrySkipPreviousAsync().and_then(|a| a.get());
            }
        }
        Cmd::Seek(sec) => {
            if let Ok(s) = mgr.GetCurrentSession() {
                let ticks = (sec * 1e7) as i64;
                let _ = s.TryChangePlaybackPositionAsync(ticks).and_then(|a| a.get());
            }
        }
        Cmd::Open(paths) => open_local(&paths, player),
    }
}

const EXTS: [&str; 6] = ["mp3", "flac", "m4a", "aac", "wav", "wma"];

fn scan(p: &Path, out: &mut Vec<PathBuf>, depth: u32) {
    if out.len() >= 2000 {
        return;
    }
    if p.is_dir() {
        if depth > 6 {
            return;
        }
        let Ok(rd) = std::fs::read_dir(p) else { return };
        let mut items: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        items.sort();
        for it in items {
            scan(&it, out, depth + 1);
        }
    } else if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
        if EXTS.iter().any(|x| x.eq_ignore_ascii_case(ext)) {
            out.push(p.to_path_buf());
        }
    }
}

fn open_local(paths: &[PathBuf], player: &mut Option<MediaPlayer>) {
    let mut files: Vec<PathBuf> = Vec::new();
    for p in paths {
        scan(p, &mut files, 0);
    }
    if files.is_empty() {
        return;
    }
    if let Some(old) = player.take() {
        let _ = old.Pause();
    }
    let Ok(list) = MediaPlaybackList::new() else { return };
    let _ = list.SetAutoRepeatEnabled(true);
    let Ok(items) = list.Items() else { return };
    let Ok(pl) = MediaPlayer::new() else { return };

    let mut started = false;
    for f in files.iter().take(1500) {
        let h = HSTRING::from(&*f.to_string_lossy());
        let Ok(file) = StorageFile::GetFileFromPathAsync(&h).and_then(|a| a.get()) else { continue };
        let Ok(src) = MediaSource::CreateFromStorageFile(&file) else { continue };
        let Ok(item) = MediaPlaybackItem::Create(&src) else { continue };
        if items.Append(&item).is_err() {
            continue;
        }
        if !started {
            // mulai putar begitu lagu pertama siap; sisanya ditambahkan sambil jalan
            started = true;
            let _ = pl.SetSource(&list);
            let _ = pl.Play();
            *player = Some(pl.clone());
        }
    }
}

fn read_thumb(p: &Props) -> Option<Vec<u8>> {
    let r = p.Thumbnail().ok()?;
    let stream = r.OpenReadAsync().ok()?.get().ok()?;
    let size = stream.Size().ok()? as u32;
    if size == 0 || size > 8_000_000 {
        return None;
    }
    let reader = DataReader::CreateDataReader(&stream).ok()?;
    reader.LoadAsync(size).ok()?.get().ok()?;
    let mut buf = vec![0u8; size as usize];
    reader.ReadBytes(&mut buf).ok()?;
    Some(buf)
}

fn poll(mgr: &Manager, shared: &Mutex<Now>, hwnd: isize, w: &mut Worker) {
    let session = mgr.GetCurrentSession().ok();
    let Some(s) = session else {
        let mut g = lock(shared);
        if g.has_session {
            let (t, a) = (g.track_id, g.art_ver);
            *g = Now::new();
            g.track_id = t + 1;
            g.art_ver = a + 1;
            drop(g);
            w.last_key.clear();
            notify(hwnd);
        }
        return;
    };

    let playing = s
        .GetPlaybackInfo()
        .and_then(|i| i.PlaybackStatus())
        .map(|st| st == Status::Playing)
        .unwrap_or(false);

    let props = s.TryGetMediaPropertiesAsync().ok().and_then(|a| a.get().ok());
    let (title, artist) = match &props {
        Some(p) => (
            p.Title().map(|h| h.to_string()).unwrap_or_default(),
            p.Artist().map(|h| h.to_string()).unwrap_or_default(),
        ),
        None => (String::new(), String::new()),
    };
    let key = format!("{}\u{1}{}", title, artist);
    let new_track = key != w.last_key;
    let had_thumb = lock(shared).thumb.is_some();
    if new_track {
        w.tries = 0;
    }
    let mut thumb: Option<Vec<u8>> = None;
    if let Some(p) = &props {
        if new_track || (!had_thumb && w.tries < 8) {
            w.tries += 1;
            thumb = read_thumb(p);
        }
    }

    let (pos, dur) = match s.GetTimelineProperties() {
        Ok(t) => {
            let ticks = |r: windows::core::Result<windows::Foundation::TimeSpan>| -> f64 {
                r.map(|x| x.Duration).unwrap_or(0) as f64 / 1e7
            };
            let p = ticks(t.Position());
            let start = ticks(t.StartTime());
            let end = ticks(t.EndTime());
            let upd = t.LastUpdatedTime().map(|d| d.UniversalTime).unwrap_or(0);
            let mut pos = p;
            if playing && upd > 0 {
                let now = (SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs_f64())
                    .unwrap_or(0.0)
                    + 11_644_473_600.0)
                    * 1e7;
                let el = (now - upd as f64) / 1e7;
                if el > 0.0 && el < 7200.0 {
                    pos += el;
                }
            }
            let dur = if end > start { end - start } else { 0.0 };
            let rel = (pos - start).max(0.0);
            (if dur > 0.0 { rel.min(dur) } else { rel }, dur)
        }
        Err(_) => (0.0, 0.0),
    };

    let mut changed = false;
    {
        let mut g = lock(shared);
        if !g.has_session || g.playing != playing {
            changed = true;
        }
        if new_track {
            g.title = title;
            g.artist = artist;
            g.thumb = None;
            g.track_id += 1;
            g.art_ver += 1;
            w.last_key = key;
            changed = true;
        }
        if let Some(t) = thumb {
            g.thumb = Some(Arc::new(t));
            g.art_ver += 1;
            changed = true;
        }
        g.has_session = true;
        g.playing = playing;
        g.pos = pos;
        g.dur = dur;
        g.stamp = Instant::now();
    }
    if changed {
        notify(hwnd);
    }
}
