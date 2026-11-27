//! Signal analysis for the music visualizer: a Hann-windowed radix-2 FFT turned into logarithmic
//! frequency bands (bass → treble) plus a decimated waveform snapshot and the RMS level.
//!
//! No allocation happens per frame: every buffer is created once in [`Analyzer::new`].
//! This module knows nothing about Windows, capture or drawing — it only turns samples into
//! numbers, so it can be fed by any audio source.

pub const FFT_N: usize = 2048;
/// Number of frequency bands published (renderers resample to however many bars they draw).
pub const BANDS: usize = 96;
/// Points of the published waveform.
pub const WAVE: usize = 256;

/// One analysed frame, all values normalised: bands 0..1 (a perceptual dB scale), wave -1..1.
#[derive(Clone)]
pub struct Frame {
    pub bands: [f32; BANDS],
    pub wave: [f32; WAVE],
    pub level: f32,
}

impl Default for Frame {
    fn default() -> Self { Frame { bands: [0.0; BANDS], wave: [0.0; WAVE], level: 0.0 } }
}

pub struct Analyzer {
    ring: Vec<f32>,
    pos: usize,
    filled: usize,
    since: usize,
    hop: usize,
    window: Vec<f32>,
    cos: Vec<f32>,
    sin: Vec<f32>,
    rev: Vec<u16>,
    re: Vec<f32>,
    im: Vec<f32>,
    /// (first bin, last bin, frequency, tilt in dB) per band — rebuilt when the sample rate changes.
    map: Vec<(usize, usize, f32)>,
    rate: u32,
    sum_sq: f32,
    sum_n: usize,
}

impl Analyzer {
    pub fn new() -> Self {
        let n = FFT_N;
        let window = (0..n).map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / (n as f32 - 1.0)).cos()).collect();
        let cos = (0..n / 2).map(|k| (std::f32::consts::TAU * k as f32 / n as f32).cos()).collect();
        let sin = (0..n / 2).map(|k| -(std::f32::consts::TAU * k as f32 / n as f32).sin()).collect();
        let bits = n.trailing_zeros();
        let rev = (0..n).map(|i| ((i as u32).reverse_bits() >> (32 - bits)) as u16).collect();
        let mut a = Analyzer { ring: vec![0.0; n], pos: 0, filled: 0, since: 0, hop: 800, window, cos, sin, rev, re: vec![0.0; n], im: vec![0.0; n], map: Vec::new(), rate: 0, sum_sq: 0.0, sum_n: 0 };
        a.set_rate(48_000);
        a
    }

    /// (Re)computes which FFT bins feed each band for this sample rate: 40 Hz … 16 kHz, log-spaced.
    pub fn set_rate(&mut self, rate: u32) {
        if rate == self.rate || rate < 8000 { return; }
        self.rate = rate;
        self.hop = (rate / 60).max(256) as usize; // ~60 analysis frames per second
        let (f_lo, f_hi) = (40.0f32, 16_000.0f32.min(rate as f32 * 0.45));
        let bin_hz = rate as f32 / FFT_N as f32;
        self.map = (0..BANDS).map(|b| {
            let a = f_lo * (f_hi / f_lo).powf(b as f32 / BANDS as f32);
            let z = f_lo * (f_hi / f_lo).powf((b + 1) as f32 / BANDS as f32);
            let first = ((a / bin_hz).floor() as usize).clamp(1, FFT_N / 2 - 1);
            let last = ((z / bin_hz).ceil() as usize).clamp(first, FFT_N / 2 - 1);
            (first, last, (a * z).sqrt())
        }).collect();
    }

    /// Feeds mono samples (-1..1). Calls `out` once per completed hop with a fresh frame.
    pub fn push(&mut self, samples: &[f32], mut out: impl FnMut(&Frame)) {
        for &s in samples {
            self.ring[self.pos] = s;
            self.pos = (self.pos + 1) % FFT_N;
            self.filled = (self.filled + 1).min(FFT_N);
            self.sum_sq += s * s;
            self.sum_n += 1;
            self.since += 1;
            if self.since >= self.hop && self.filled >= FFT_N {
                self.since = 0;
                let mut f = Frame::default();
                self.analyse(&mut f);
                out(&f);
            }
        }
    }

    fn analyse(&mut self, f: &mut Frame) {
        let n = FFT_N;
        // Windowed, bit-reversed copy of the last N samples (oldest first).
        for i in 0..n {
            let s = self.ring[(self.pos + i) % n] * self.window[i];
            let r = self.rev[i] as usize;
            self.re[r] = s;
            self.im[r] = 0.0;
        }
        // Iterative radix-2 butterflies.
        let mut size = 2;
        while size <= n {
            let half = size / 2;
            let step = n / size;
            for start in (0..n).step_by(size) {
                for k in 0..half {
                    let (wr, wi) = (self.cos[k * step], self.sin[k * step]);
                    let (a, b) = (start + k, start + k + half);
                    let (tr, ti) = (self.re[b] * wr - self.im[b] * wi, self.re[b] * wi + self.im[b] * wr);
                    self.re[b] = self.re[a] - tr;
                    self.im[b] = self.im[a] - ti;
                    self.re[a] += tr;
                    self.im[a] += ti;
                }
            }
            size <<= 1;
        }
        // Magnitude → amplitude (Hann coherent gain 0.5) → dB → 0..1, with a +3 dB/octave tilt above
        // 200 Hz so treble isn't invisible next to bass (music is roughly pink).
        let scale = 4.0 / n as f32;
        for (b, &(first, last, fc)) in self.map.iter().enumerate() {
            let mut m = 0.0f32;
            for k in first..=last { m = m.max(self.re[k] * self.re[k] + self.im[k] * self.im[k]); }
            let amp = m.sqrt() * scale;
            let tilt = 3.0 * (fc / 200.0).max(1.0).log2();
            let db = 20.0 * (amp + 1e-9).log10() + tilt;
            f.bands[b] = ((db + 72.0) / 62.0).clamp(0.0, 1.0);
        }
        // Waveform: the newest 2·WAVE samples, averaged in pairs.
        for i in 0..WAVE {
            let a = self.ring[(self.pos + n - 2 * WAVE + 2 * i) % n];
            let b = self.ring[(self.pos + n - 2 * WAVE + 2 * i + 1) % n];
            f.wave[i] = ((a + b) * 0.5).clamp(-1.0, 1.0);
        }
        f.level = if self.sum_n > 0 { (self.sum_sq / self.sum_n as f32).sqrt() } else { 0.0 };
        self.sum_sq = 0.0;
        self.sum_n = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pure 1 kHz sine must light the band containing 1 kHz and leave the far bands quiet.
    #[test]
    fn sine_lands_in_the_right_band() {
        let rate = 48_000u32;
        let mut a = Analyzer::new();
        a.set_rate(rate);
        let tone: Vec<f32> = (0..FFT_N * 4).map(|i| 0.5 * (std::f32::consts::TAU * 1000.0 * i as f32 / rate as f32).sin()).collect();
        let mut last = Frame::default();
        a.push(&tone, |f| last = f.clone());
        let peak = last.bands.iter().enumerate().max_by(|x, y| x.1.partial_cmp(y.1).unwrap()).unwrap().0;
        let f_lo = 40.0f32; let f_hi = 16_000.0f32;
        let centre = f_lo * (f_hi / f_lo).powf((peak as f32 + 0.5) / BANDS as f32);
        assert!((centre / 1000.0 - 1.0).abs() < 0.12, "peak band centre {centre} Hz");
        assert!(last.bands[0] < 0.2 && last.bands[BANDS - 1] < 0.2, "far bands should be quiet");
        assert!(last.level > 0.3 && last.level < 0.4, "rms of a 0.5 sine is ~0.354, got {}", last.level);
    }

    /// Silence stays silent.
    #[test]
    fn silence_is_zero() {
        let mut a = Analyzer::new();
        let mut last = Frame::default();
        let mut got = false;
        a.push(&vec![0.0; FFT_N * 2], |f| { last = f.clone(); got = true; });
        assert!(got && last.bands.iter().all(|&b| b == 0.0) && last.level == 0.0);
    }
}
