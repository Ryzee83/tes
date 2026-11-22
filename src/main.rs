#![windows_subsystem = "windows"]

//! LumaBeat - pemutar/pendamping musik estetik super ringan untuk Windows 10/11.
//! Cahaya tepi layar mengikuti warna cover & beat, player kaca, lirik tersinkron.

mod audio;
mod gfx;
mod glow;
mod lyrics;
mod media;
mod palette;
mod panel;
mod settings;
mod tray;

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gfx::wide;
use windows_sys::Win32::Foundation::{GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::HiDpi::{GetDpiForSystem, SetProcessDpiAwarenessContext};
use windows_sys::Win32::UI::Shell::{DragFinish, DragQueryFileW, SHQueryUserNotificationState};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub const WM_APP_MEDIA: u32 = WM_APP + 1;
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

// id menu
const ID_GLOW: u32 = 1;
const ID_PLAYER: u32 = 2;
const ID_LYRICS: u32 = 3;
const ID_THIN: u32 = 10;
const ID_MEDIUM: u32 = 11;
const ID_THICK: u32 = 12;
const ID_ALLSCREENS: u32 = 20;
const ID_HIDEFS: u32 = 21;
const ID_AUTOSTART: u32 = 22;
const ID_OPEN: u32 = 30;
const ID_QUIT: u32 = 99;

thread_local! {
    static APP: RefCell<Option<Box<App>>> = RefCell::new(None);
}

fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|c| match c.try_borrow_mut() {
        Ok(mut g) => g.as_mut().map(|a| f(a)),
        Err(_) => None,
    })
}

struct App {
    hwnd: HWND,
    hinst: HINSTANCE,
    cls_ov: *const u16,
    s: f32,
    cfg: settings::Settings,
    shared: Arc<Mutex<media::Now>>,
    tx: Sender<media::Cmd>,
    tap: Option<audio::Tap>,
    tap_tried: Option<Instant>,
    strips: Vec<glow::Strip>,
    panel: panel::Panel,
    lyr: panel::LyricsWin,
    pal: Vec<palette::Rgb>,
    lut: [[u8; 3]; 256],
    art: Option<(Vec<u8>, i32)>,
    tray_art: Option<Vec<u8>>,
    tray_icon: isize,
    // dinamika cahaya
    phase: f32,
    level: f32,
    pulse: f32,
    refl: f32,
    avg: f32,
    intensity: f32,
    // pembukuan
    last_tick: Instant,
    tick_n: u64,
    timer_on: bool,
    interval: u32,
    track_id: u64,
    art_ver: u64,
    fs_hidden: bool,
    on_battery: bool,
    lyric_slot: Arc<Mutex<(u64, Option<Arc<lyrics::Lyrics>>)>>,
    lyrics: Option<Arc<lyrics::Lyrics>>,
    lyrics_for: u64,
    fetching_for: u64,
    lyr_idx: Option<usize>,
    lyr_anim: f32,
    lyr_dirty: bool,
    panel_key: (u64, u64, bool, i32),
}

fn work_area() -> RECT {
    let mut r = RECT { left: 0, top: 0, right: 1280, bottom: 720 };
    unsafe {
        SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut r as *mut RECT as *mut _, 0);
    }
    r
}

fn clamp_pos(x: i32, y: i32, w: i32, h: i32) -> (i32, i32) {
    unsafe {
        let vx = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let vy = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let vw = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let vh = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        (x.clamp(vx, (vx + vw - w).max(vx)), y.clamp(vy, (vy + vh - h).max(vy)))
    }
}

fn ease(x: f32) -> f32 {
    1.0 - (1.0 - x).powi(3)
}

impl App {
    fn new(hwnd: HWND, hinst: HINSTANCE, cls_ov: *const u16, cls_panel: *const u16) -> Option<App> {
        let s = (unsafe { GetDpiForSystem() }.max(96)) as f32 / 96.0;
        let cfg = settings::Settings::load();
        let work = work_area();
        let pw = (panel::PW * s).round() as i32;
        let ph = (panel::PH * s).round() as i32;
        let pos = if cfg.has_pos {
            clamp_pos(cfg.px, cfg.py, pw, ph)
        } else {
            let m = (24.0 * s) as i32;
            (work.right - pw - m, work.bottom - ph - m)
        };
        let panel = panel::Panel::new(hinst, cls_panel, s, pos)?;
        let lyr = panel::LyricsWin::new(hinst, cls_ov, s, &work)?;

        let shared = Arc::new(Mutex::new(media::Now::new()));
        let tx = media::spawn(hwnd, shared.clone());

        let pal = palette::default_palette();
        let lut = palette::build_lut(&pal);
        let icon = tray::make_hicon(&tray::icon_pixels(None, &pal, 32), 32);
        tray::add(hwnd, icon, "LumaBeat");

        Some(App {
            hwnd,
            hinst,
            cls_ov,
            s,
            cfg,
            shared,
            tx,
            tap: None,
            tap_tried: None,
            strips: Vec::new(),
            panel,
            lyr,
            pal,
            lut,
            art: None,
            tray_art: None,
            tray_icon: icon,
            phase: 0.0,
            level: 0.25,
            pulse: 0.0,
            refl: 0.05,
            avg: 0.0,
            intensity: 0.0,
            last_tick: Instant::now(),
            tick_n: 0,
            timer_on: false,
            interval: 33,
            track_id: 0,
            art_ver: 0,
            fs_hidden: false,
            on_battery: false,
            lyric_slot: Arc::new(Mutex::new((u64::MAX, None))),
            lyrics: None,
            lyrics_for: u64::MAX,
            fetching_for: u64::MAX,
            lyr_idx: None,
            lyr_anim: 1.0,
            lyr_dirty: false,
            panel_key: (u64::MAX, 0, false, -1),
        })
    }

    fn snapshot(&self) -> media::Now {
        media::lock(&self.shared).clone()
    }

    // ------------------------------------------------------------ timer

    fn ensure_timer(&mut self) {
        if !self.timer_on {
            self.timer_on = true;
            self.last_tick = Instant::now();
            unsafe {
                SetTimer(self.hwnd, 1, self.interval, None);
            }
        }
    }

    fn restart_timer(&mut self) {
        self.timer_on = true;
        unsafe {
            SetTimer(self.hwnd, 1, self.interval, None);
        }
    }

    fn stop_timer(&mut self) {
        if self.timer_on {
            unsafe {
                KillTimer(self.hwnd, 1);
            }
            self.timer_on = false;
        }
    }

    // ------------------------------------------------------------ lingkungan

    /// Layar penuh? baterai? Dicek jarang saja (sekitar 1,5 detik sekali).
    fn check_env(&mut self) {
        unsafe {
            let mut st: i32 = 5;
            let hr = SHQueryUserNotificationState(&mut st);
            self.fs_hidden = self.cfg.hide_fullscreen && hr >= 0 && matches!(st, 2 | 3 | 4);
            let mut sp: SYSTEM_POWER_STATUS = std::mem::zeroed();
            if GetSystemPowerStatus(&mut sp) != 0 {
                self.on_battery = sp.ACLineStatus == 0;
            }
        }
        let iv = if self.on_battery { 50 } else { 33 };
        if iv != self.interval {
            self.interval = iv;
            if self.timer_on {
                self.restart_timer();
            }
        }
    }

    // ------------------------------------------------------------ frame

    fn tick(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_tick).as_secs_f32().clamp(0.001, 0.25);
        self.last_tick = now;
        self.tick_n += 1;
        let snap = self.snapshot();

        if self.tick_n % 45 == 1 {
            self.check_env();
        }
        if self.tick_n % 90 == 0 {
            for st in &self.strips {
                st.raise();
            }
            self.panel.raise();
            self.lyr.raise();
        }

        self.update_audio(dt, &snap);
        self.draw_glow(&snap);
        self.update_panel(&snap, false);
        self.update_lyrics(&snap, dt);

        // tidak ada yang bergerak -> matikan timer sama sekali
        if !snap.playing && self.intensity < 0.012 && self.strips.is_empty() && self.lyr_anim >= 1.0 {
            self.stop_timer();
            self.tap = None;
        }
    }

    fn update_audio(&mut self, dt: f32, snap: &media::Now) {
        let failed = self.tap.as_ref().map_or(false, |t| t.lv.err.load(Ordering::Relaxed));
        if failed {
            self.tap = None;
        }
        if snap.playing
            && self.tap.is_none()
            && self.tap_tried.map_or(true, |t| t.elapsed() > Duration::from_secs(3))
        {
            self.tap_tried = Some(Instant::now());
            self.tap = audio::Tap::start();
        }
        let (b, f) = match &self.tap {
            Some(t) => t.take(),
            None => (0.0, 0.0),
        };

        let playing = snap.playing;
        let quiet = f < 0.0015;
        let k30 = dt * 30.0;
        self.refl = (self.refl * 0.997f32.powf(k30)).max(b).max(0.01);
        let nb = (b / self.refl).clamp(0.0, 1.0);
        self.avg += (nb - self.avg) * (1.0 - (-dt * 6.0).exp());
        let onset = ((nb - self.avg * 1.15) * 3.0).clamp(0.0, 1.0);
        self.pulse = (self.pulse * 0.80f32.powf(k30)).max(onset);

        let (target, speed) = if !playing || quiet {
            (0.25, 8.0)
        } else {
            (0.30 + 0.40 * nb + 0.50 * self.pulse, 10.0 + 60.0 * nb + 90.0 * self.pulse)
        };
        self.level += (target - self.level) * (1.0 - (-dt * 12.0).exp());
        self.phase = (self.phase + speed * dt) % 256.0;

        let tgt = if playing && self.cfg.glow && !self.fs_hidden { 1.0 } else { 0.0 };
        self.intensity += (tgt - self.intensity) * (1.0 - (-dt * 3.5).exp());
    }

    fn build_strips(&mut self) {
        let base = [36.0f32, 72.0, 120.0][self.cfg.thickness.min(2) as usize];
        let t = (base * self.s) as i32;
        for r in glow::monitors() {
            if !self.cfg.all_screens && !(r.left == 0 && r.top == 0) {
                continue;
            }
            for kind in 0..4u8 {
                if let Some(st) = glow::Strip::new(self.hinst, self.cls_ov, r, kind, t) {
                    self.strips.push(st);
                }
            }
        }
    }

    fn draw_glow(&mut self, snap: &media::Now) {
        let want = self.cfg.glow && !self.fs_hidden && snap.playing;
        if !want && self.intensity < 0.012 {
            // sudah padam total: buang jendela & memori
            if !self.strips.is_empty() {
                self.strips.clear();
            }
            return;
        }
        if self.strips.is_empty() {
            self.build_strips();
        }
        let inten = ((self.level.clamp(0.0, 1.0) * self.intensity) * 256.0) as u32;
        let phase = self.phase as u8;
        for st in &mut self.strips {
            st.render(&self.lut, phase, inten.min(256));
        }
    }

    // ------------------------------------------------------------ lagu / cover

    fn load_art(&mut self, snap: &media::Now) {
        self.art = None;
        self.tray_art = None;
        self.pal = palette::default_palette();
        if let Some(bytes) = &snap.thumb {
            if let Ok(img) = image::load_from_memory(bytes.as_slice()) {
                let small = img.thumbnail(48, 48).to_rgba8();
                self.pal = palette::extract(small.as_raw());
                let sz = (panel::ART * self.s).round() as u32;
                let big = img.resize_to_fill(sz, sz, image::imageops::FilterType::Triangle).to_rgba8();
                self.art = Some((big.into_raw(), sz as i32));
                let ic = img.resize_to_fill(32, 32, image::imageops::FilterType::Triangle).to_rgba8();
                self.tray_art = Some(ic.into_raw());
            }
        }
        self.lut = palette::build_lut(&self.pal);
    }

    fn update_tray(&mut self, snap: &media::Now) {
        let px = tray::icon_pixels(self.tray_art.as_deref(), &self.pal, 32);
        let icon = tray::make_hicon(&px, 32);
        let tip = if snap.title.is_empty() {
            "LumaBeat".to_string()
        } else if snap.artist.is_empty() {
            snap.title.clone()
        } else {
            format!("{} - {}", snap.artist, snap.title)
        };
        tray::modify(self.hwnd, icon, &tip);
        if self.tray_icon != 0 {
            unsafe {
                DestroyIcon(self.tray_icon);
            }
        }
        self.tray_icon = icon;
    }

    fn sync_track(&mut self, snap: &media::Now) {
        if snap.track_id != self.track_id {
            self.track_id = snap.track_id;
            self.lyrics = None;
            self.lyrics_for = u64::MAX;
            self.lyr_idx = None;
            self.lyr_anim = 1.0;
            self.lyr_dirty = true;
        }
        if snap.art_ver != self.art_ver {
            self.art_ver = snap.art_ver;
            self.load_art(snap);
            self.update_tray(snap);
        }
    }

    /// Dipanggil saat thread media / thread lirik memberi kabar.
    fn on_media(&mut self) {
        let snap = self.snapshot();
        self.sync_track(&snap);
        self.update_panel(&snap, true);
        self.update_lyrics(&snap, 0.0);
        if snap.playing || self.intensity > 0.012 || self.lyr_anim < 1.0 {
            self.ensure_timer();
        }
    }

    fn update_panel(&mut self, snap: &media::Now, force: bool) {
        let show = self.cfg.player && snap.has_session && !self.fs_hidden;
        if !show {
            self.panel.hide();
            return;
        }
        let pos = snap.position_now();
        let key = (snap.track_id, self.art_ver, snap.playing, pos as i32);
        if !force && self.panel.shown && self.panel_key == key {
            return;
        }
        self.panel_key = key;
        let view = panel::PanelView {
            title: &snap.title,
            artist: &snap.artist,
            playing: snap.playing,
            pos,
            dur: snap.dur,
            art: self.art.as_ref(),
            pal: &self.pal,
        };
        self.panel.render(&view);
        self.panel.show();
    }

    fn update_lyrics(&mut self, snap: &media::Now, dt: f32) {
        let show = self.cfg.lyrics && snap.playing && !self.fs_hidden && !snap.title.is_empty();
        if !show {
            self.lyr.hide();
            return;
        }
        if self.fetching_for != snap.track_id {
            self.fetching_for = snap.track_id;
            let slot = self.lyric_slot.clone();
            let id = snap.track_id;
            let (t, a, d) = (snap.title.clone(), snap.artist.clone(), snap.dur);
            let hwnd = self.hwnd;
            std::thread::spawn(move || {
                let ly = lyrics::fetch(&t, &a, d).map(Arc::new);
                *media::lock(&slot) = (id, ly);
                unsafe {
                    PostMessageW(hwnd, WM_APP_MEDIA, 0, 0);
                }
            });
        }
        if self.lyrics_for != snap.track_id {
            let g = media::lock(&self.lyric_slot);
            if g.0 == snap.track_id {
                self.lyrics = g.1.clone();
                self.lyrics_for = snap.track_id;
                self.lyr_idx = None;
                self.lyr_dirty = true;
            }
        }
        let Some(ly) = self.lyrics.clone() else {
            self.lyr.hide();
            return;
        };

        let pos = snap.position_now();
        let idx = ly.index_at(pos + 0.25);
        if idx != self.lyr_idx {
            self.lyr_idx = idx;
            self.lyr_anim = 0.0;
            self.lyr_dirty = true;
        } else if self.lyr_anim < 1.0 {
            self.lyr_anim = (self.lyr_anim + dt * 4.5).min(1.0);
            self.lyr_dirty = true;
        }
        if !self.lyr_dirty && self.lyr.shown {
            return;
        }
        self.lyr_dirty = false;

        let cur = idx.map(|i| ly.lines[i].1.as_str()).unwrap_or("");
        let nxt = ly
            .lines
            .get(idx.map(|i| i + 1).unwrap_or(0))
            .map(|l| l.1.as_str())
            .unwrap_or("");
        let accent = self.pal.first().copied().unwrap_or([200, 160, 255]);
        self.lyr.render(cur, nxt, ease(self.lyr_anim), accent);
        self.lyr.show();
    }

    // ------------------------------------------------------------ input

    fn panel_click(&mut self, x: i32, y: i32) {
        use media::Cmd;
        match self.panel.hit(x, y) {
            panel::Hit::Prev => {
                let _ = self.tx.send(Cmd::Prev);
            }
            panel::Hit::Play => {
                let _ = self.tx.send(Cmd::PlayPause);
            }
            panel::Hit::Next => {
                let _ = self.tx.send(Cmd::Next);
            }
            panel::Hit::Bar => {
                let snap = self.snapshot();
                if snap.dur > 0.0 {
                    let _ = self.tx.send(Cmd::Seek(self.panel.bar_frac(x) * snap.dur));
                }
            }
            panel::Hit::None => {}
        }
    }

    fn command(&mut self, id: u32) {
        match id {
            ID_GLOW => self.cfg.glow = !self.cfg.glow,
            ID_PLAYER => self.cfg.player = !self.cfg.player,
            ID_LYRICS => self.cfg.lyrics = !self.cfg.lyrics,
            ID_THIN | ID_MEDIUM | ID_THICK => {
                self.cfg.thickness = (id - ID_THIN) as u8;
                self.strips.clear();
            }
            ID_ALLSCREENS => {
                self.cfg.all_screens = !self.cfg.all_screens;
                self.strips.clear();
            }
            ID_HIDEFS => self.cfg.hide_fullscreen = !self.cfg.hide_fullscreen,
            ID_AUTOSTART => {
                self.cfg.autostart = !self.cfg.autostart;
                set_autostart(self.cfg.autostart);
            }
            ID_OPEN => {
                unsafe {
                    AllowSetForegroundWindow(u32::MAX);
                }
                pick_folder(self.tx.clone());
                return;
            }
            ID_QUIT => {
                unsafe {
                    DestroyWindow(self.hwnd);
                }
                return;
            }
            _ => return,
        }
        self.cfg.save();
        self.check_env();
        let snap = self.snapshot();
        self.update_panel(&snap, true);
        self.update_lyrics(&snap, 0.0);
        if snap.playing || self.intensity > 0.012 {
            self.ensure_timer();
        }
    }
}

// ---------------------------------------------------------------- util

fn set_autostart(on: bool) {
    use std::os::windows::process::CommandExt;
    let key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    let mut c = std::process::Command::new("reg");
    if on {
        let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default();
        let val = format!("\"{}\"", exe);
        c.args(["add", key, "/v", "LumaBeat", "/t", "REG_SZ", "/d", val.as_str(), "/f"]);
    } else {
        c.args(["delete", key, "/v", "LumaBeat", "/f"]);
    }
    let _ = c.creation_flags(0x0800_0000).spawn();
}

/// Dialog pilih folder lewat PowerShell (ada di semua Windows 10/11) di thread terpisah.
fn pick_folder(tx: Sender<media::Cmd>) {
    std::thread::spawn(move || {
        use std::os::windows::process::CommandExt;
        let script = "[Console]::OutputEncoding=[Text.Encoding]::UTF8; \
            Add-Type -AssemblyName System.Windows.Forms; \
            $d=New-Object System.Windows.Forms.FolderBrowserDialog; \
            $d.Description='Pilih folder musik'; $d.ShowNewFolderButton=$false; \
            if($d.ShowDialog() -eq 'OK'){[Console]::Out.Write($d.SelectedPath)}";
        let out = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-STA", "-WindowStyle", "Hidden", "-Command", script])
            .creation_flags(0x0800_0000)
            .output();
        if let Ok(out) = out {
            let s = String::from_utf8_lossy(&out.stdout)
                .trim_matches(|c: char| c == '\u{feff}' || c.is_whitespace())
                .to_string();
            if !s.is_empty() {
                let _ = tx.send(media::Cmd::Open(vec![PathBuf::from(s)]));
            }
        }
    });
}

unsafe fn add_item(m: HMENU, id: u32, text: &str, checked: bool) {
    let t = wide(text);
    let flags = MF_STRING | if checked { MF_CHECKED } else { 0 };
    AppendMenuW(m, flags, id as usize, t.as_ptr());
}

unsafe fn show_menu(main: HWND) {
    let Some(cfg) = with_app(|a| a.cfg.clone()) else { return };
    let menu = CreatePopupMenu();
    add_item(menu, ID_GLOW, "Edge light", cfg.glow);
    add_item(menu, ID_PLAYER, "Desktop player", cfg.player);
    add_item(menu, ID_LYRICS, "Synced lyrics", cfg.lyrics);

    let sub = CreatePopupMenu();
    add_item(sub, ID_THIN, "Thin", cfg.thickness == 0);
    add_item(sub, ID_MEDIUM, "Medium", cfg.thickness == 1);
    add_item(sub, ID_THICK, "Thick", cfg.thickness == 2);
    let label = wide("Light thickness");
    AppendMenuW(menu, MF_POPUP | MF_STRING, sub as usize, label.as_ptr());

    add_item(menu, ID_ALLSCREENS, "Light on all screens", cfg.all_screens);
    add_item(menu, ID_HIDEFS, "Step aside in full screen", cfg.hide_fullscreen);
    AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
    add_item(menu, ID_OPEN, "Play music folder...", false);
    add_item(menu, ID_AUTOSTART, "Start with Windows", cfg.autostart);
    AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
    add_item(menu, ID_QUIT, "Quit", false);

    let mut pt = POINT { x: 0, y: 0 };
    GetCursorPos(&mut pt);
    SetForegroundWindow(main);
    let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN, pt.x, pt.y, 0, main, std::ptr::null());
    PostMessageW(main, WM_NULL, 0, 0);
    DestroyMenu(menu);
    if cmd > 0 {
        with_app(|a| a.command(cmd as u32));
    }
}

// ---------------------------------------------------------------- window procs

fn lo(lp: LPARAM) -> i32 {
    (lp & 0xFFFF) as u16 as i16 as i32
}

fn hi(lp: LPARAM) -> i32 {
    ((lp >> 16) & 0xFFFF) as u16 as i16 as i32
}

unsafe extern "system" fn main_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let tc = TASKBAR_CREATED.load(Ordering::Relaxed);
    if tc != 0 && msg == tc {
        with_app(|a| tray::add(a.hwnd, a.tray_icon, "LumaBeat"));
        return 0;
    }
    match msg {
        WM_TIMER => {
            if wp == 1 {
                with_app(|a| a.tick());
            }
            0
        }
        WM_APP_MEDIA => {
            with_app(|a| a.on_media());
            0
        }
        tray::WM_TRAY => {
            let ev = (lp & 0xFFFF) as u32;
            if ev == WM_RBUTTONUP || ev == WM_LBUTTONUP {
                show_menu(hwnd);
            }
            0
        }
        WM_DISPLAYCHANGE => {
            with_app(|a| a.strips.clear());
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe extern "system" fn ov_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    DefWindowProcW(hwnd, msg, wp, lp)
}

unsafe extern "system" fn panel_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_NCHITTEST => {
            let (x, y) = (lo(lp), hi(lp));
            let mut r: RECT = std::mem::zeroed();
            GetWindowRect(hwnd, &mut r);
            let hit = with_app(|a| a.panel.hit(x - r.left, y - r.top)).unwrap_or(panel::Hit::None);
            if hit == panel::Hit::None {
                HTCAPTION as LRESULT
            } else {
                HTCLIENT as LRESULT
            }
        }
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_LBUTTONUP => {
            let (x, y) = (lo(lp), hi(lp));
            with_app(|a| a.panel_click(x, y));
            0
        }
        WM_RBUTTONUP | WM_NCRBUTTONUP => {
            if let Some(main) = with_app(|a| a.hwnd) {
                show_menu(main);
            }
            0
        }
        WM_EXITSIZEMOVE => {
            let mut r: RECT = std::mem::zeroed();
            GetWindowRect(hwnd, &mut r);
            with_app(|a| {
                a.cfg.px = r.left;
                a.cfg.py = r.top;
                a.cfg.has_pos = true;
                a.cfg.save();
            });
            0
        }
        WM_DROPFILES => {
            let hdrop = wp as isize;
            let n = DragQueryFileW(hdrop, 0xFFFF_FFFF, std::ptr::null_mut(), 0);
            let mut paths: Vec<PathBuf> = Vec::new();
            for i in 0..n {
                let len = DragQueryFileW(hdrop, i, std::ptr::null_mut(), 0);
                let mut buf = vec![0u16; len as usize + 1];
                DragQueryFileW(hdrop, i, buf.as_mut_ptr(), len + 1);
                paths.push(PathBuf::from(String::from_utf16_lossy(&buf[..len as usize])));
            }
            DragFinish(hdrop);
            if !paths.is_empty() {
                with_app(|a| {
                    let _ = a.tx.send(media::Cmd::Open(paths));
                });
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

// ---------------------------------------------------------------- main

fn leak_wide(s: &str) -> *const u16 {
    Box::leak(wide(s).into_boxed_slice()).as_ptr()
}

unsafe fn register_class(name: *const u16, proc_: WNDPROC, hinst: HINSTANCE) {
    let mut wc: WNDCLASSW = std::mem::zeroed();
    wc.lpfnWndProc = proc_;
    wc.hInstance = hinst;
    wc.lpszClassName = name;
    wc.hCursor = LoadCursorW(0, IDC_ARROW);
    RegisterClassW(&wc);
}

fn main() {
    unsafe {
        // satu instance saja
        let _mutex = CreateMutexW(std::ptr::null(), 0, wide("Local\\LumaBeat.SingleInstance").as_ptr());
        if GetLastError() == 183 {
            return; // ERROR_ALREADY_EXISTS
        }

        SetProcessDpiAwarenessContext(-4); // per-monitor v2

        let hinst: HINSTANCE = GetModuleHandleW(std::ptr::null());
        let cls_main = leak_wide("LumaBeatMain");
        let cls_ov = leak_wide("LumaBeatOverlay");
        let cls_panel = leak_wide("LumaBeatPanel");
        register_class(cls_main, Some(main_proc), hinst);
        register_class(cls_ov, Some(ov_proc), hinst);
        register_class(cls_panel, Some(panel_proc), hinst);
        TASKBAR_CREATED.store(RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()), Ordering::Relaxed);

        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            cls_main,
            cls_main,
            WS_POPUP,
            0,
            0,
            0,
            0,
            0,
            0,
            hinst,
            std::ptr::null(),
        );
        if hwnd == 0 {
            return;
        }

        match App::new(hwnd, hinst, cls_ov, cls_panel) {
            Some(app) => APP.with(|c| *c.borrow_mut() = Some(Box::new(app))),
            None => return,
        }
        with_app(|a| a.check_env());

        // "Open with" / seret file ke exe -> langsung putar
        let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
        if !args.is_empty() {
            with_app(|a| {
                let _ = a.tx.send(media::Cmd::Open(args));
            });
        }

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, 0, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        tray::remove(hwnd);
        APP.with(|c| *c.borrow_mut() = None);
    }
}
