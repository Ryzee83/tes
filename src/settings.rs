//! Pengaturan disimpan sebagai teks sederhana: %APPDATA%\LumaBeat\settings.ini

use std::path::PathBuf;

#[derive(Clone)]
pub struct Settings {
    pub glow: bool,
    pub player: bool,
    pub lyrics: bool,
    pub thickness: u8, // 0 tipis, 1 sedang, 2 tebal
    pub all_screens: bool,
    pub hide_fullscreen: bool,
    pub autostart: bool,
    pub px: i32,
    pub py: i32,
    pub has_pos: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            glow: true,
            player: true,
            lyrics: true,
            thickness: 1,
            all_screens: true,
            hide_fullscreen: true,
            autostart: false,
            px: 0,
            py: 0,
            has_pos: false,
        }
    }
}

fn dir() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")?;
    let mut p = PathBuf::from(base);
    p.push("LumaBeat");
    Some(p)
}

impl Settings {
    pub fn load() -> Settings {
        let mut s = Settings::default();
        let Some(mut p) = dir() else { return s };
        p.push("settings.ini");
        let Ok(txt) = std::fs::read_to_string(&p) else { return s };
        let (mut seen_x, mut seen_y) = (false, false);
        for line in txt.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim());
            let b = v == "1";
            match k {
                "glow" => s.glow = b,
                "player" => s.player = b,
                "lyrics" => s.lyrics = b,
                "thickness" => s.thickness = v.parse::<u8>().unwrap_or(1).min(2),
                "all_screens" => s.all_screens = b,
                "hide_fullscreen" => s.hide_fullscreen = b,
                "autostart" => s.autostart = b,
                "px" => {
                    if let Ok(n) = v.parse() {
                        s.px = n;
                        seen_x = true;
                    }
                }
                "py" => {
                    if let Ok(n) = v.parse() {
                        s.py = n;
                        seen_y = true;
                    }
                }
                _ => {}
            }
        }
        s.has_pos = seen_x && seen_y;
        s
    }

    pub fn save(&self) {
        let Some(mut p) = dir() else { return };
        let _ = std::fs::create_dir_all(&p);
        p.push("settings.ini");
        let b = |x: bool| if x { 1 } else { 0 };
        let mut t = String::new();
        t.push_str(&format!("glow={}\n", b(self.glow)));
        t.push_str(&format!("player={}\n", b(self.player)));
        t.push_str(&format!("lyrics={}\n", b(self.lyrics)));
        t.push_str(&format!("thickness={}\n", self.thickness));
        t.push_str(&format!("all_screens={}\n", b(self.all_screens)));
        t.push_str(&format!("hide_fullscreen={}\n", b(self.hide_fullscreen)));
        t.push_str(&format!("autostart={}\n", b(self.autostart)));
        if self.has_pos {
            t.push_str(&format!("px={}\npy={}\n", self.px, self.py));
        }
        let _ = std::fs::write(&p, t);
    }
}
