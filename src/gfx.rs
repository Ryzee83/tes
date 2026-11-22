//! Renderer perangkat lunak mini + pembungkus GDI.
//! Semua piksel premultiplied 0xAARRGGBB (format yang diminta UpdateLayeredWindow).

use std::ffi::c_void;
use std::ptr::null_mut;

use crate::palette::Rgb;
use windows_sys::Win32::Foundation::{HWND, POINT, RECT, SIZE};
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject, DrawTextW, GdiFlush,
    SelectObject, SetBkMode, SetTextColor, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION,
    DIB_RGB_COLORS, DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX, DT_RIGHT, DT_SINGLELINE,
    DT_TOP, HBITMAP, HDC, HFONT, HGDIOBJ, TRANSPARENT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{UpdateLayeredWindow, ULW_ALPHA};

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

// ---------------------------------------------------------------- DIB

pub struct Dib {
    pub dc: HDC,
    bmp: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u32,
    pub w: i32,
    pub h: i32,
}

impl Dib {
    pub fn new(w: i32, h: i32) -> Option<Dib> {
        if w <= 0 || h <= 0 {
            return None;
        }
        unsafe {
            let dc = CreateCompatibleDC(0);
            if dc == 0 {
                return None;
            }
            let mut bi: BITMAPINFO = std::mem::zeroed();
            bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            bi.bmiHeader.biWidth = w;
            bi.bmiHeader.biHeight = -h; // top-down
            bi.bmiHeader.biPlanes = 1;
            bi.bmiHeader.biBitCount = 32;
            bi.bmiHeader.biCompression = BI_RGB;
            let mut bits: *mut c_void = null_mut();
            let bmp = CreateDIBSection(dc, &bi, DIB_RGB_COLORS, &mut bits, 0, 0);
            if bmp == 0 || bits.is_null() {
                DeleteDC(dc);
                return None;
            }
            let old = SelectObject(dc, bmp);
            Some(Dib { dc, bmp, old, bits: bits as *mut u32, w, h })
        }
    }

    pub fn pixels(&mut self) -> &mut [u32] {
        unsafe { std::slice::from_raw_parts_mut(self.bits, (self.w * self.h) as usize) }
    }
}

impl Drop for Dib {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.old);
            DeleteObject(self.bmp);
            DeleteDC(self.dc);
        }
    }
}

/// Tampilkan DIB ke jendela layered. `pos = None` -> posisi jendela tidak diubah.
pub fn present(hwnd: HWND, dib: &Dib, pos: Option<(i32, i32)>) {
    unsafe {
        let pt = POINT { x: pos.map(|p| p.0).unwrap_or(0), y: pos.map(|p| p.1).unwrap_or(0) };
        let pptdst: *const POINT = if pos.is_some() { &pt } else { std::ptr::null() };
        let sz = SIZE { cx: dib.w, cy: dib.h };
        let src = POINT { x: 0, y: 0 };
        let bf = BLENDFUNCTION { BlendOp: 0, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: 1 };
        UpdateLayeredWindow(hwnd, 0, pptdst, &sz, dib.dc, &src, 0, &bf, ULW_ALPHA);
    }
}

// ---------------------------------------------------------------- Canvas

fn sd_rrect(px: f32, py: f32, x0: f32, y0: f32, x1: f32, y1: f32, r: f32) -> f32 {
    let r = r.min((x1 - x0) * 0.5).min((y1 - y0) * 0.5).max(0.0);
    let cx = (x0 + x1) * 0.5;
    let cy = (y0 + y1) * 0.5;
    let hx = (x1 - x0) * 0.5 - r;
    let hy = (y1 - y0) * 0.5 - r;
    let qx = (px - cx).abs() - hx;
    let qy = (py - cy).abs() - hy;
    let ox = qx.max(0.0);
    let oy = qy.max(0.0);
    (ox * ox + oy * oy).sqrt() + qx.max(qy).min(0.0) - r
}

fn inside(pts: &[(f32, f32)], px: f32, py: f32) -> bool {
    let mut c = false;
    let n = pts.len();
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = pts[i];
        let (xj, yj) = pts[j];
        if ((yi > py) != (yj > py)) && (px < (xj - xi) * (py - yi) / (yj - yi) + xi) {
            c = !c;
        }
        j = i;
    }
    c
}

pub struct Canvas {
    pub w: i32,
    pub h: i32,
    pub px: Vec<u32>,
}

impl Canvas {
    pub fn new(w: i32, h: i32) -> Canvas {
        Canvas { w, h, px: vec![0; (w.max(1) * h.max(1)) as usize] }
    }

    pub fn clear(&mut self) {
        self.px.fill(0);
    }

    /// Campur warna lurus `c` dengan alpha `a` (0..=255) ke piksel (x, y).
    #[inline]
    pub fn blend(&mut self, x: i32, y: i32, c: Rgb, a: u32) {
        if a == 0 || x < 0 || y < 0 || x >= self.w || y >= self.h {
            return;
        }
        let a = a.min(255);
        let d = &mut self.px[(y * self.w + x) as usize];
        let ia = 255 - a;
        let da = (*d >> 24) & 255;
        let dr = (*d >> 16) & 255;
        let dg = (*d >> 8) & 255;
        let db = *d & 255;
        let oa = a + da * ia / 255;
        let or = c[0] as u32 * a / 255 + dr * ia / 255;
        let og = c[1] as u32 * a / 255 + dg * ia / 255;
        let ob = c[2] as u32 * a / 255 + db * ia / 255;
        *d = (oa.min(255) << 24) | (or.min(255) << 16) | (og.min(255) << 8) | ob.min(255);
    }

    pub fn fill_rrect_with<F: Fn(i32, i32) -> Rgb>(
        &mut self,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        r: f32,
        col: F,
        alpha: u32,
    ) {
        let bx0 = (x0.floor() as i32 - 1).max(0);
        let by0 = (y0.floor() as i32 - 1).max(0);
        let bx1 = (x1.ceil() as i32 + 1).min(self.w);
        let by1 = (y1.ceil() as i32 + 1).min(self.h);
        for y in by0..by1 {
            for x in bx0..bx1 {
                let d = sd_rrect(x as f32 + 0.5, y as f32 + 0.5, x0, y0, x1, y1, r);
                let cov = (0.5 - d).clamp(0.0, 1.0);
                if cov > 0.0 {
                    let c = col(x, y);
                    self.blend(x, y, c, (cov * alpha as f32) as u32);
                }
            }
        }
    }

    pub fn fill_rrect(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, r: f32, col: Rgb, alpha: u32) {
        self.fill_rrect_with(x0, y0, x1, y1, r, |_, _| col, alpha);
    }

    pub fn stroke_rrect(
        &mut self,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        r: f32,
        width: f32,
        col: Rgb,
        alpha: u32,
    ) {
        let bx0 = (x0.floor() as i32 - 1).max(0);
        let by0 = (y0.floor() as i32 - 1).max(0);
        let bx1 = (x1.ceil() as i32 + 1).min(self.w);
        let by1 = (y1.ceil() as i32 + 1).min(self.h);
        for y in by0..by1 {
            for x in bx0..bx1 {
                let d = sd_rrect(x as f32 + 0.5, y as f32 + 0.5, x0, y0, x1, y1, r);
                let cov = (width * 0.5 + 0.5 - (d + width * 0.5).abs()).clamp(0.0, 1.0);
                if cov > 0.0 {
                    self.blend(x, y, col, (cov * alpha as f32) as u32);
                }
            }
        }
    }

    /// Gambar RGBA lurus berukuran `size` x `size` dengan sudut membulat.
    pub fn draw_image(&mut self, img: &[u8], size: i32, x0: i32, y0: i32, r: f32) {
        for y in 0..size {
            for x in 0..size {
                let d = sd_rrect(x as f32 + 0.5, y as f32 + 0.5, 0.0, 0.0, size as f32, size as f32, r);
                let cov = (0.5 - d).clamp(0.0, 1.0);
                if cov <= 0.0 {
                    continue;
                }
                let i = ((y * size + x) * 4) as usize;
                if i + 3 >= img.len() {
                    return;
                }
                let a = (img[i + 3] as f32 * cov) as u32;
                self.blend(x0 + x, y0 + y, [img[i], img[i + 1], img[i + 2]], a);
            }
        }
    }

    /// Poligon sederhana dengan supersampling 3x3.
    pub fn fill_poly(&mut self, pts: &[(f32, f32)], col: Rgb, alpha: u32) {
        if pts.len() < 3 {
            return;
        }
        let minx = pts.iter().map(|p| p.0).fold(f32::MAX, f32::min).floor() as i32;
        let maxx = pts.iter().map(|p| p.0).fold(f32::MIN, f32::max).ceil() as i32;
        let miny = pts.iter().map(|p| p.1).fold(f32::MAX, f32::min).floor() as i32;
        let maxy = pts.iter().map(|p| p.1).fold(f32::MIN, f32::max).ceil() as i32;
        for y in miny..=maxy {
            for x in minx..=maxx {
                let mut hit = 0u32;
                for sy in 0..3 {
                    for sx in 0..3 {
                        let px = x as f32 + (sx as f32 + 0.5) / 3.0;
                        let py = y as f32 + (sy as f32 + 0.5) / 3.0;
                        if inside(pts, px, py) {
                            hit += 1;
                        }
                    }
                }
                if hit > 0 {
                    self.blend(x, y, col, alpha * hit / 9);
                }
            }
        }
    }
}

// ---------------------------------------------------------------- Teks (GDI -> mask alpha)

pub struct TextDrawer {
    dib: Dib,
    fonts: Vec<(i32, i32, HFONT)>,
}

impl TextDrawer {
    pub fn new(w: i32, h: i32) -> Option<TextDrawer> {
        Some(TextDrawer { dib: Dib::new(w.max(8), h.max(8))?, fonts: Vec::new() })
    }

    fn font(&mut self, px: i32, weight: i32) -> HFONT {
        if let Some(f) = self.fonts.iter().find(|f| f.0 == px && f.1 == weight) {
            return f.2;
        }
        let face = wide("Segoe UI");
        // tinggi negatif = tinggi karakter piksel; charset 1 = DEFAULT; kualitas 4 = ANTIALIASED
        let f = unsafe { CreateFontW(-px.max(6), 0, 0, 0, weight, 0, 0, 0, 1, 0, 0, 4, 0, face.as_ptr()) };
        self.fonts.push((px, weight, f));
        f
    }

    /// Gambar teks 1 baris di kotak (x, y, w, h). align: 0 kiri, 1 tengah, 2 kanan.
    /// GDI menggambar putih di atas hitam; kanal hijau dipakai sebagai cakupan alpha.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        cv: &mut Canvas,
        text: &str,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        px: i32,
        weight: i32,
        col: Rgb,
        alpha: u32,
        align: i32,
    ) {
        if text.is_empty() || alpha == 0 {
            return;
        }
        let w = w.min(self.dib.w);
        let h = h.min(self.dib.h);
        if w <= 0 || h <= 0 {
            return;
        }
        let font = self.font(px, weight);
        unsafe {
            let stride = self.dib.w;
            let n = (stride * h) as usize;
            std::slice::from_raw_parts_mut(self.dib.bits, n).fill(0);
            let old = SelectObject(self.dib.dc, font);
            SetBkMode(self.dib.dc, 1); // 1 = TRANSPARENT
            SetTextColor(self.dib.dc, 0x00FF_FFFF);
            let mut rc = RECT { left: 0, top: 0, right: w, bottom: h };
            let mut flags = DT_NOPREFIX | DT_SINGLELINE | DT_END_ELLIPSIS | DT_TOP;
            flags |= match align {
                1 => DT_CENTER,
                2 => DT_RIGHT,
                _ => DT_LEFT,
            };
            let wt = wide(text);
            DrawTextW(self.dib.dc, wt.as_ptr(), -1, &mut rc, flags);
            GdiFlush();
            SelectObject(self.dib.dc, old);
            let pix = std::slice::from_raw_parts(self.dib.bits as *const u32, n);
            for yy in 0..h {
                for xx in 0..w {
                    let g = (pix[(yy * stride + xx) as usize] >> 8) & 0xFF;
                    if g > 0 {
                        cv.blend(x + xx, y + yy, col, g * alpha / 255);
                    }
                }
            }
        }
    }
}

impl Drop for TextDrawer {
    fn drop(&mut self) {
        for f in &self.fonts {
            unsafe {
                DeleteObject(f.2);
            }
        }
    }
}
