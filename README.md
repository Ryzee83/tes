# LumaBeat

Pendamping/pemutar musik estetik super ringan untuk **Windows 10 & 11**, ditulis dengan **Rust**.
Implementasi mandiri (bukan salinan kode/aset produk mana pun).

## Fitur

- **Cahaya tepi layar** dengan warna dari cover album, bergerak mengikuti beat (bass).
  Dikirim lewat 4 jendela tipis per monitor, bukan overlay layar penuh.
- **Player kaca** di desktop: cover, judul, artis, progress (klik untuk seek), prev / play-pause / next. Bisa diseret.
- **Lirik tersinkron** (LRCLIB) yang naik baris demi baris di bawah layar.
- **Ikon di dekat jam** berubah jadi cover album bulat; klik untuk menu pengaturan.
- **Mengikuti musik di mana saja**: Spotify, Apple Music, YouTube Music / SoundCloud di browser,
  atau aplikasi apa pun yang mendukung kontrol media Windows (SMTC).
- **Putar file sendiri**: seret file/folder ke player, atau menu tray -> *Play music folder...*,
  atau `lumabeat.exe "D:\Musik"`. Format: mp3, flac, m4a, aac, wav, wma (codec bawaan Windows).

## Hemat sumber daya

- Saat tidak ada lagu yang diputar: **tidak ada jendela cahaya, tidak ada timer, tidak ada capture audio**.
- Mask cahaya dihitung sekali; per frame hanya perkalian warna sederhana.
- Beat dideteksi dengan filter low-pass (tanpa FFT). Audio **tidak direkam** dan bukan dari mikrofon
  (hanya loopback keluaran sistem untuk menghitung level).
- Di baterai: 20 fps (di listrik: 30 fps). Otomatis menyingkir saat aplikasi layar penuh.

## Cara build

1. Install Rust: https://rustup.rs (pilih toolchain `stable-x86_64-pc-windows-msvc`).
   Jika diminta, install *Build Tools for Visual Studio* dengan workload **Desktop development with C++**.
2. Buka terminal di folder ini lalu jalankan:

```
cargo build --release
```

3. Hasilnya: `target\release\lumabeat.exe` (CRT statis, tidak perlu VC++ Redistributable).

## Installer tanpa setup Rust (build di cloud)

Tidak perlu install Rust di komputer Anda. GitHub membangun `lumabeat.exe` dan `LumaBeat-Setup.exe` untuk Anda:

1. Buat akun GitHub gratis, lalu buat repository baru (boleh *Private*).
2. Upload **seluruh isi** folder proyek ini (termasuk folder `.github` dan `installer`).
3. Buka tab **Actions** -> workflow **Build installer** (berjalan otomatis setelah upload, atau klik *Run workflow*).
4. Setelah centang hijau (sekitar 5-10 menit), buka run tersebut -> bagian **Artifacts** -> unduh **LumaBeat-Setup**.
5. Ekstrak zip-nya, jalankan `LumaBeat-Setup.exe`. Installer ini per-user (tanpa admin).

Jika build gagal, buka langkah yang merah di log Actions dan salin pesan errornya.
Windows SmartScreen bisa memperingatkan karena installer belum ditandatangani: klik *More info* -> *Run anyway*.

## Catatan

- Lirik memakai `curl.exe` (bawaan Windows 10 1803+ / 11). Dialog folder memakai PowerShell (bawaan Windows).
- Pengaturan tersimpan di `%APPDATA%\LumaBeat\settings.ini`.
- Hanya satu instance yang berjalan. Untuk keluar: klik kanan ikon tray -> *Quit*.
- Belum ada: wallpaper bergerak, pencarian katalog, cover-flow album.
