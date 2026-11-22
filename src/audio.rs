//! Menangkap audio keluaran sistem (WASAPI loopback lewat cpal) - BUKAN mikrofon.
//! Hanya menghitung energi bass + energi total (low-pass 2 kutub, tanpa FFT).
//! Tidak ada audio yang disimpan atau dikirim ke mana pun.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

#[derive(Default)]
pub struct Levels {
    bass: AtomicU32,
    full: AtomicU32,
    pub err: AtomicBool,
}

pub struct Tap {
    _stream: cpal::Stream,
    pub lv: Arc<Levels>,
}

struct Dsp {
    a: f32,
    lp1: f32,
    lp2: f32,
    ch: usize,
}

impl Dsp {
    fn new(sr: f32, ch: usize) -> Dsp {
        let fc = 140.0f32; // batas bass (Hz)
        let a = 1.0 - (-2.0 * std::f32::consts::PI * fc / sr.max(8000.0)).exp();
        Dsp { a, lp1: 0.0, lp2: 0.0, ch: ch.max(1) }
    }

    fn feed<I: Iterator<Item = f32>>(&mut self, it: I, lv: &Levels) {
        let mut acc_b = 0.0f32;
        let mut acc_f = 0.0f32;
        let mut n = 0u32;
        let mut sum = 0.0f32;
        let mut k = 0usize;
        for s in it {
            sum += s;
            k += 1;
            if k == self.ch {
                let m = sum / self.ch as f32;
                sum = 0.0;
                k = 0;
                self.lp1 += self.a * (m - self.lp1);
                self.lp2 += self.a * (self.lp1 - self.lp2);
                acc_b += self.lp2 * self.lp2;
                acc_f += m * m;
                n += 1;
            }
        }
        if n == 0 {
            return;
        }
        let b = (acc_b / n as f32).sqrt();
        let f = (acc_f / n as f32).sqrt();
        // float non-negatif: urutan bit == urutan nilai, jadi fetch_max aman
        if b.is_finite() {
            lv.bass.fetch_max(b.to_bits(), Ordering::Relaxed);
        }
        if f.is_finite() {
            lv.full.fetch_max(f.to_bits(), Ordering::Relaxed);
        }
    }
}

impl Tap {
    pub fn start() -> Option<Tap> {
        let host = cpal::default_host();
        let dev = host.default_output_device()?;
        let cfg = dev.default_output_config().ok()?;
        let sr = cfg.sample_rate().0 as f32;
        let ch = cfg.channels() as usize;
        let fmt = cfg.sample_format();
        let conf: cpal::StreamConfig = cfg.into();

        let lv = Arc::new(Levels::default());
        let l_data = lv.clone();
        let l_err = lv.clone();
        let mut dsp = Dsp::new(sr, ch);
        let err_fn = move |_e: cpal::StreamError| {
            l_err.err.store(true, Ordering::Relaxed);
        };

        // Membuka device OUTPUT sebagai input = loopback di Windows (WASAPI).
        let stream = match fmt {
            cpal::SampleFormat::F32 => dev.build_input_stream(
                &conf,
                move |d: &[f32], _: &cpal::InputCallbackInfo| dsp.feed(d.iter().copied(), &l_data),
                err_fn,
                None,
            ),
            cpal::SampleFormat::I16 => dev.build_input_stream(
                &conf,
                move |d: &[i16], _: &cpal::InputCallbackInfo| {
                    dsp.feed(d.iter().map(|&s| s as f32 / 32768.0), &l_data)
                },
                err_fn,
                None,
            ),
            _ => return None,
        }
        .ok()?;
        stream.play().ok()?;
        Some(Tap { _stream: stream, lv })
    }

    /// (bass_peak, full_peak) sejak panggilan terakhir.
    pub fn take(&self) -> (f32, f32) {
        let b = f32::from_bits(self.lv.bass.swap(0, Ordering::Relaxed));
        let f = f32::from_bits(self.lv.full.swap(0, Ordering::Relaxed));
        (b, f)
    }
}
