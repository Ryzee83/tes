//! Ikon di dekat jam (system tray). Ikon berubah jadi cover album (bulat) berbingkai
//! cincin warna album.

use std::ffi::c_void;
use std::ptr::null_mut;

use crate::palette::{build_lut, Rgb};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Graphics::Gdi::{
    CreateBitmap, CreateDIBSection, DeleteObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
};
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, HICON, ICONINFO, WM_APP};

pub const WM_TRAY: u32 = WM_APP + 2;

/// Piksel ikon premultiplied BGRA berukuran `size` x `size`.
/// `art` = RGBA lurus (size x size) atau None.
pub fn icon_pixels(art: Option<&[u8]>, pal: &[Rgb], size: i32) -> Vec<u32> {
    let n = size as usize;
    let mut out = vec![0u32; n * n];
    let lut = build_lut(pal);
    let c = (size as f32 - 1.0) * 0.5;
    let outer = size as f32 * 0.5 - 0.5;
    let inner = outer - 3.0;
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - c;
            let dy = y as f32 - c;
            let d = (dx * dx + dy * dy).sqrt();
            let cov = (outer + 0.5 - d).clamp(0.0, 1.0);
            if cov <= 0.0 {
                continue;
            }
            let col: Rgb = if d <= inner {
                match art {
                    Some(a) => {
                        let i = ((y * size + x) * 4) as usize;
                        if i + 2 < a.len() {
                            [a[i], a[i + 1], a[i + 2]]
                        } else {
                            [24, 24, 34]
                        }
                    }
                    None => [24, 24, 34],
                }
            } else {
                let t = dy.atan2(dx) / std::f32::consts::TAU + 0.5;
                lut[((t * 255.0) as usize).min(255)]
            };
            let a = (cov * 255.0) as u32;
            let r = col[0] as u32 * a / 255;
            let g = col[1] as u32 * a / 255;
            let b = col[2] as u32 * a / 255;
            out[(y * size + x) as usize] = (a << 24) | (r << 16) | (g << 8) | b;
        }
    }
    out
}

pub fn make_hicon(px: &[u32], size: i32) -> HICON {
    unsafe {
        let mut bi: BITMAPINFO = std::mem::zeroed();
        bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bi.bmiHeader.biWidth = size;
        bi.bmiHeader.biHeight = -size;
        bi.bmiHeader.biPlanes = 1;
        bi.bmiHeader.biBitCount = 32;
        bi.bmiHeader.biCompression = BI_RGB;
        let mut bits: *mut c_void = null_mut();
        let color = CreateDIBSection(0, &bi, DIB_RGB_COLORS, &mut bits, 0, 0);
        if color == 0 || bits.is_null() {
            return 0;
        }
        let count = px.len().min((size * size) as usize);
        std::ptr::copy_nonoverlapping(px.as_ptr(), bits as *mut u32, count);
        let mask_bits = vec![0u8; (((size + 15) / 16) * 2 * size) as usize];
        let mask = CreateBitmap(size, size, 1, 1, mask_bits.as_ptr() as *const c_void);
        let ii = ICONINFO { fIcon: 1, xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: color };
        let icon = CreateIconIndirect(&ii);
        DeleteObject(color);
        DeleteObject(mask);
        icon
    }
}

fn nid(hwnd: HWND, icon: HICON, tip: &str) -> NOTIFYICONDATAW {
    let mut d: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    d.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    d.hWnd = hwnd;
    d.uID = 1;
    d.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    d.uCallbackMessage = WM_TRAY;
    d.hIcon = icon;
    for (i, ch) in tip.encode_utf16().take(127).enumerate() {
        d.szTip[i] = ch;
    }
    d
}

pub fn add(hwnd: HWND, icon: HICON, tip: &str) {
    let d = nid(hwnd, icon, tip);
    unsafe {
        Shell_NotifyIconW(NIM_ADD, &d);
    }
}

pub fn modify(hwnd: HWND, icon: HICON, tip: &str) {
    let d = nid(hwnd, icon, tip);
    unsafe {
        Shell_NotifyIconW(NIM_MODIFY, &d);
    }
}

pub fn remove(hwnd: HWND) {
    let d = nid(hwnd, 0, "");
    unsafe {
        Shell_NotifyIconW(NIM_DELETE, &d);
    }
}
