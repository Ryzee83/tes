//! Cahaya tepi layar.
//! Tiap monitor = 4 jendela tipis (atas, bawah, kiri, kanan), click-through,
//! sehingga yang digambar hanya piksel di pinggir (bukan seluruh layar).
//! Mask (seberapa terang) dan posisi warna dihitung SEKALI saat dibuat;
//! per frame hanya mengalikan warna dari LUT x intensitas.

use crate::gfx::{present, Dib};
use windows_sys::Win32::Foundation::{HINSTANCE, HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub struct Strip {
    hwnd: HWND,
    x: i32,
    y: i32,
    dib: Dib,
    mask: Vec<u8>,
    ang: Vec<u8>,
    shown: bool,
}

unsafe extern "system" fn mon_cb(_m: HMONITOR, _dc: HDC, r: *mut RECT, lp: isize) -> i32 {
    let v = &mut *(lp as *mut Vec<RECT>);
    v.push(*r);
    1
}

pub fn monitors() -> Vec<RECT> {
    let mut v: Vec<RECT> = Vec::new();
    unsafe {
        EnumDisplayMonitors(0, std::ptr::null(), Some(mon_cb), &mut v as *mut Vec<RECT> as isize);
    }
    v
}

impl Strip {
    /// kind: 0 atas, 1 bawah, 2 kiri, 3 kanan. `t` = ketebalan cahaya (px).
    pub fn new(hinst: HINSTANCE, class: *const u16, mon: RECT, kind: u8, t: i32) -> Option<Strip> {
        let mw = mon.right - mon.left;
        let mh = mon.bottom - mon.top;
        let (ox, oy, w, h) = match kind {
            0 => (0, 0, mw, t),
            1 => (0, mh - t, mw, t),
            2 => (0, t, t, mh - 2 * t),
            _ => (mw - t, t, t, mh - 2 * t),
        };
        if w <= 0 || h <= 0 || t < 4 {
            return None;
        }

        // profil jatuh cahaya terhadap jarak dari tepi
        let tu = t as usize;
        let mut fall = vec![0u8; tu];
        for (d, slot) in fall.iter_mut().enumerate() {
            let v = 1.0 - d as f32 / t as f32;
            let glow = v.powf(2.4) * 0.85;
            let core = (1.0 - d as f32 / 2.5).max(0.0) * 0.6;
            *slot = ((glow + core).min(1.0) * 255.0) as u8;
        }

        let rad = ((t as f32 * 0.5) as i32).max(12);
        let per = 2.0 * (mw + mh) as f32;
        let n = (w * h) as usize;
        let mut mask = vec![0u8; n];
        let mut ang = vec![0u8; n];
        for py in 0..h {
            for px in 0..w {
                let gx = ox + px;
                let gy = oy + py;
                let dx = gx.min(mw - 1 - gx);
                let dy = gy.min(mh - 1 - gy);
                let d = if dx < rad && dy < rad {
                    let cx = (rad - dx) as f32;
                    let cy = (rad - dy) as f32;
                    (rad as f32 - (cx * cx + cy * cy).sqrt()).max(0.0)
                } else {
                    dx.min(dy) as f32
                };
                let i = (py * w + px) as usize;
                let di = d as usize;
                mask[i] = if di < tu { fall[di] } else { 0 };
                // posisi sepanjang keliling layar -> warna mengalir mengelilingi layar
                let p = if dy <= dx {
                    if gy < mh / 2 {
                        gx as f32
                    } else {
                        (mw + mh) as f32 + (mw - 1 - gx) as f32
                    }
                } else if gx < mw / 2 {
                    (2 * mw + mh) as f32 + (mh - 1 - gy) as f32
                } else {
                    mw as f32 + gy as f32
                };
                ang[i] = ((p / per) * 256.0) as u32 as u8;
            }
        }

        let dib = Dib::new(w, h)?;
        let (x, y) = (mon.left + ox, mon.top + oy);
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
        Some(Strip { hwnd, x, y, dib, mask, ang, shown: false })
    }

    /// `inten` 0..=256, `phase` menggeser warna di sepanjang keliling.
    pub fn render(&mut self, lut: &[[u8; 3]; 256], phase: u8, inten: u32) {
        {
            let buf = self.dib.pixels();
            for i in 0..buf.len() {
                let a = (self.mask[i] as u32 * inten) >> 8;
                if a == 0 {
                    buf[i] = 0;
                    continue;
                }
                let c = lut[self.ang[i].wrapping_add(phase) as usize];
                let r = (c[0] as u32 * a) >> 8;
                let g = (c[1] as u32 * a) >> 8;
                let b = (c[2] as u32 * a) >> 8;
                buf[i] = (a << 24) | (r << 16) | (g << 8) | b;
            }
        }
        present(self.hwnd, &self.dib, Some((self.x, self.y)));
        if !self.shown {
            unsafe {
                ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
            }
            self.shown = true;
        }
    }

    pub fn raise(&self) {
        unsafe {
            SetWindowPos(self.hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }
}

impl Drop for Strip {
    fn drop(&mut self) {
        unsafe {
            DestroyWindow(self.hwnd);
        }
    }
}
