//! Player "kaca" di desktop + jendela lirik. Digambar dengan renderer mini (gfx.rs).

use crate::gfx::{present, Canvas, Dib, TextDrawer};
use crate::palette::Rgb;
use windows_sys::Win32::Foundation::{HINSTANCE, HWND, RECT};
use windows_sys::Win32::UI::Shell::DragAcceptFiles;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

// ukuran logis (dikali skala DPI)
pub const PW: f32 = 380.0;
pub const PH: f32 = 140.0;
pub const ART: f32 = 108.0;

const PREV: (f32, f32) = (196.0, 114.0);
const PLAY: (f32, f32) = (258.0, 114.0);
const NEXT: (f32, f32) = (320.0, 114.0);
const BAR_X0: f32 = 140.0;
const BAR_X1: f32 = 364.0;
const BAR_Y: f32 = 76.0;

const WHITE: Rgb = [255, 255, 255];
const DARK: Rgb = [18, 18, 26];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    None,
    Prev,
    Play,
    Next,
    Bar,
}

pub struct PanelView<'a> {
    pub title: &'a str,
    pub artist: &'a str,
    pub playing: bool,
    pub pos: f64,
    pub dur: f64,
    pub art: Option<&'a (Vec<u8>, i32)>,
    pub pal: &'a [Rgb],
}

fn lerp(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let f = |i: usize| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t).clamp(0.0, 255.0) as u8;
    [f(0), f(1), f(2)]
}

fn tint(c: Rgb, k: f32) -> Rgb {
    let f = |i: usize| (c[i] as f32 * k + 12.0).min(255.0) as u8;
    [f(0), f(1), f(2)]
}

fn mmss(sec: f64) -> String {
    let s = sec.max(0.0) as i64;
    format!("{}:{:02}", s / 60, s % 60)
}

fn draw_skip(cv: &mut Canvas, cx: f32, cy: f32, s: f32, prev: bool) {
    let d = if prev { -1.0f32 } else { 1.0f32 };
    let (b0, b1) = if prev { (cx - 8.0 * s, cx - 6.0 * s) } else { (cx + 6.0 * s, cx + 8.0 * s) };
    cv.fill_rrect(b0, cy - 7.0 * s, b1, cy + 7.0 * s, 0.8 * s, WHITE, 235);
    cv.fill_poly(
        &[
            (cx + d * 6.0 * s, cy),
            (cx - d * 7.0 * s, cy - 7.0 * s),
            (cx - d * 7.0 * s, cy + 7.0 * s),
        ],
        WHITE,
        235,
    );
}

pub struct Panel {
    pub hwnd: HWND,
    pub shown: bool,
    s: f32,
    w: i32,
    h: i32,
    dib: Dib,
    cv: Canvas,
    td: TextDrawer,
}

impl Panel {
    pub fn new(hinst: HINSTANCE, class: *const u16, s: f32, pos: (i32, i32)) -> Option<Panel> {
        let w = (PW * s).round() as i32;
        let h = (PH * s).round() as i32;
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class,
                std::ptr::null(),
                WS_POPUP,
                pos.0,
                pos.1,
                w,
                h,
                0,
                0,
                hinst,
                std::ptr::null(),
            )
        };
        if hwnd == 0 {
            return None;
        }
        unsafe {
            DragAcceptFiles(hwnd, 1);
        }
        let dib = Dib::new(w, h)?;
        let td = TextDrawer::new(w, (30.0 * s) as i32)?;
        Some(Panel { hwnd, shown: false, s, w, h, dib, cv: Canvas::new(w, h), td })
    }

    pub fn hit(&self, x: i32, y: i32) -> Hit {
        let (x, y) = (x as f32 / self.s, y as f32 / self.s);
        let near = |c: (f32, f32)| ((x - c.0).powi(2) + (y - c.1).powi(2)).sqrt() < 20.0;
        if near(PREV) {
            Hit::Prev
        } else if near(PLAY) {
            Hit::Play
        } else if near(NEXT) {
            Hit::Next
        } else if x >= BAR_X0 - 4.0 && x <= BAR_X1 + 4.0 && y >= BAR_Y - 8.0 && y <= BAR_Y + 10.0 {
            Hit::Bar
        } else {
            Hit::None
        }
    }

    pub fn bar_frac(&self, x: i32) -> f64 {
        (((x as f32 / self.s) - BAR_X0) / (BAR_X1 - BAR_X0)).clamp(0.0, 1.0) as f64
    }

    pub fn show(&mut self) {
        if !self.shown {
            unsafe {
                ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
            }
            self.shown = true;
        }
    }

    pub fn hide(&mut self) {
        if self.shown {
            unsafe {
                ShowWindow(self.hwnd, SW_HIDE);
            }
            self.shown = false;
        }
    }

    pub fn raise(&self) {
        if self.shown {
            unsafe {
                SetWindowPos(self.hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            }
        }
    }

    pub fn render(&mut self, v: &PanelView) {
        let s = self.s;
        let (fw, fh) = (self.w as f32, self.h as f32);
        {
            let cv = &mut self.cv;
            let td = &mut self.td;
            cv.clear();

            let p0 = v.pal.first().copied().unwrap_or([140, 96, 255]);
            let p1 = v.pal.get(1).copied().unwrap_or(p0);
            let c0 = tint(p0, 0.30);
            let c1 = tint(p1, 0.20);
            let r = 22.0 * s;

            // kaca: gradient gelap bertint warna album, semi transparan
            cv.fill_rrect_with(
                0.5,
                0.5,
                fw - 0.5,
                fh - 0.5,
                r,
                |x, y| {
                    let t = ((x as f32 / fw) * 0.6 + (y as f32 / fh) * 0.4).clamp(0.0, 1.0);
                    lerp(c0, c1, t)
                },
                226,
            );
            cv.stroke_rrect(0.5, 0.5, fw - 0.5, fh - 0.5, r, 1.2 * s, WHITE, 60);

            // cover
            let ax = (16.0 * s).round() as i32;
            match v.art {
                Some((rgba, sz)) => cv.draw_image(rgba, *sz, ax, ax, 14.0 * s),
                None => {
                    let sz = ART * s;
                    cv.fill_rrect(ax as f32, ax as f32, ax as f32 + sz, ax as f32 + sz, 14.0 * s, lerp(p0, WHITE, 0.2), 90);
                    td.draw(
                        cv,
                        "\u{266A}",
                        ax,
                        ax + (26.0 * s) as i32,
                        sz as i32,
                        (30.0 * s) as i32,
                        (34.0 * s) as i32,
                        400,
                        WHITE,
                        200,
                        1,
                    );
                }
            }

            // judul + artis
            let tx = (140.0 * s).round() as i32;
            let tw = (226.0 * s).round() as i32;
            let title = if v.title.is_empty() { "Unknown" } else { v.title };
            td.draw(cv, title, tx, (13.0 * s) as i32, tw, (26.0 * s) as i32, (17.0 * s) as i32, 700, WHITE, 255, 0);
            td.draw(cv, v.artist, tx, (38.0 * s) as i32, tw, (22.0 * s) as i32, (13.0 * s) as i32, 400, WHITE, 170, 0);

            // progress
            let (bx0, bx1, by) = (BAR_X0 * s, BAR_X1 * s, BAR_Y * s);
            cv.fill_rrect(bx0, by, bx1, by + 4.0 * s, 2.0 * s, WHITE, 55);
            let frac = if v.dur > 0.0 { (v.pos / v.dur).clamp(0.0, 1.0) as f32 } else { 0.0 };
            if frac > 0.0 {
                cv.fill_rrect(bx0, by, bx0 + (bx1 - bx0) * frac, by + 4.0 * s, 2.0 * s, lerp(p0, WHITE, 0.35), 255);
            }
            let tw2 = (80.0 * s) as i32;
            td.draw(cv, &mmss(v.pos), tx, (84.0 * s) as i32, tw2, (16.0 * s) as i32, (11.0 * s) as i32, 400, WHITE, 140, 0);
            if v.dur > 0.0 {
                td.draw(cv, &mmss(v.dur), tx + tw - tw2, (84.0 * s) as i32, tw2, (16.0 * s) as i32, (11.0 * s) as i32, 400, WHITE, 140, 2);
            }

            // tombol
            let cy = PLAY.1 * s;
            draw_skip(cv, PREV.0 * s, cy, s, true);
            draw_skip(cv, NEXT.0 * s, cy, s, false);
            let cx = PLAY.0 * s;
            cv.fill_rrect(cx - 17.0 * s, cy - 17.0 * s, cx + 17.0 * s, cy + 17.0 * s, 17.0 * s, WHITE, 240);
            if v.playing {
                cv.fill_rrect(cx - 6.0 * s, cy - 7.0 * s, cx - 2.0 * s, cy + 7.0 * s, 1.2 * s, DARK, 255);
                cv.fill_rrect(cx + 2.0 * s, cy - 7.0 * s, cx + 6.0 * s, cy + 7.0 * s, 1.2 * s, DARK, 255);
            } else {
                cv.fill_poly(
                    &[(cx - 4.0 * s, cy - 7.5 * s), (cx - 4.0 * s, cy + 7.5 * s), (cx + 7.5 * s, cy)],
                    DARK,
                    255,
                );
            }
        }
        self.dib.pixels().copy_from_slice(&self.cv.px);
        present(self.hwnd, &self.dib, None);
    }
}

impl Drop for Panel {
    fn drop(&mut self) {
        unsafe {
            DestroyWindow(self.hwnd);
        }
    }
}

// ---------------------------------------------------------------- Lirik

pub struct LyricsWin {
    pub hwnd: HWND,
    pub shown: bool,
    s: f32,
    w: i32,
    h: i32,
    dib: Dib,
    cv: Canvas,
    td: TextDrawer,
}

impl LyricsWin {
    pub fn new(hinst: HINSTANCE, class: *const u16, s: f32, work: &RECT) -> Option<LyricsWin> {
        let ww = work.right - work.left;
        let w = ((1100.0 * s) as i32).min(ww * 7 / 10).max(300);
        let h = (130.0 * s) as i32;
        let x = work.left + (ww - w) / 2;
        let y = work.bottom - h - (20.0 * s) as i32;
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class,
                std::ptr::null(),
                WS_POPUP,
                x,
                y,
                w,
                h,
                0,
                0,
                hinst,
                std::ptr::null(),
            )
        };
        if hwnd == 0 {
            return None;
        }
        let dib = Dib::new(w, h)?;
        let td = TextDrawer::new(w, (48.0 * s) as i32)?;
        Some(LyricsWin { hwnd, shown: false, s, w, h, dib, cv: Canvas::new(w, h), td })
    }

    pub fn show(&mut self) {
        if !self.shown {
            unsafe {
                ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
            }
            self.shown = true;
        }
    }

    pub fn hide(&mut self) {
        if self.shown {
            unsafe {
                ShowWindow(self.hwnd, SW_HIDE);
            }
            self.shown = false;
        }
    }

    pub fn raise(&self) {
        if self.shown {
            unsafe {
                SetWindowPos(self.hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            }
        }
    }

    /// `t` 0..1 = animasi baris baru "naik" ke tempatnya.
    pub fn render(&mut self, cur: &str, next: &str, t: f32, accent: Rgb) {
        let s = self.s;
        let w = self.w;
        {
            let cv = &mut self.cv;
            let td = &mut self.td;
            cv.clear();
            let off = ((1.0 - t) * 14.0 * s) as i32;
            let a = (255.0 * t) as u32;
            let y0 = (20.0 * s) as i32 + off;
            let hh = (46.0 * s) as i32;
            let px = (30.0 * s) as i32;
            td.draw(cv, cur, 2, y0 + 2, w - 4, hh, px, 700, [0, 0, 0], a * 6 / 10, 1);
            td.draw(cv, cur, 0, y0, w, hh, px, 700, WHITE, a, 1);
            let soft = lerp(accent, WHITE, 0.45);
            td.draw(cv, next, 0, (76.0 * s) as i32, w, (30.0 * s) as i32, (19.0 * s) as i32, 600, soft, 120, 1);
        }
        self.dib.pixels().copy_from_slice(&self.cv.px);
        present(self.hwnd, &self.dib, None);
        let _ = self.h;
    }
}

impl Drop for LyricsWin {
    fn drop(&mut self) {
        unsafe {
            DestroyWindow(self.hwnd);
        }
    }
}
