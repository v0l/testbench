use crate::waveform::Waveform;
use std::f64::consts::PI;

#[derive(Debug, Clone, PartialEq)]
pub struct Spectrum {
    pub df: f64,
    pub db: Vec<f32>,
}

impl Spectrum {
    pub fn frequency(&self, bin: usize) -> f64 {
        self.df * bin as f64
    }

    pub fn peak(&self) -> Option<(usize, f32)> {
        self.db
            .iter()
            .copied()
            .enumerate()
            .skip(1)
            .max_by(|a, b| a.1.total_cmp(&b.1))
    }
}

pub fn fft(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    assert!(n.is_power_of_two() && im.len() == n, "fft needs a power of two length");
    if n < 2 {
        return;
    }
    let bits = n.trailing_zeros();
    for i in 0..n {
        let j = i.reverse_bits() >> (usize::BITS - bits);
        if j > i {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * PI / len as f64;
        let (wr, wi) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0, 0.0);
            for k in 0..len / 2 {
                let a = start + k;
                let b = a + len / 2;
                let tr = re[b] * cr - im[b] * ci;
                let ti = re[b] * ci + im[b] * cr;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let nr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = nr;
            }
        }
        len <<= 1;
    }
}

pub fn hann(n: usize) -> Vec<f64> {
    if n < 2 {
        return vec![1.0; n];
    }
    (0..n)
        .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f64 / (n - 1) as f64).cos())
        .collect()
}

pub fn spectrum(w: &Waveform) -> Option<Spectrum> {
    if w.volts.len() < 4 || w.dt <= 0.0 {
        return None;
    }
    let n = 1usize << (usize::BITS - 1 - w.volts.len().leading_zeros());
    let mean = w.volts[..n].iter().map(|&v| f64::from(v)).sum::<f64>() / n as f64;
    let window = hann(n);
    let gain: f64 = window.iter().sum::<f64>() / n as f64;
    let mut re: Vec<f64> = w.volts[..n]
        .iter()
        .zip(&window)
        .map(|(&v, wi)| (f64::from(v) - mean) * wi)
        .collect();
    let mut im = vec![0.0; n];
    fft(&mut re, &mut im);
    let db = (0..=n / 2)
        .map(|k| {
            let scale = if k == 0 || k == n / 2 { 1.0 } else { 2.0 };
            let amp = scale * re[k].hypot(im[k]) / (n as f64 * gain);
            (20.0 * amp.max(1e-12).log10()) as f32
        })
        .collect();
    Some(Spectrum { df: 1.0 / (w.dt * n as f64), db })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn impulse_is_flat() {
        let mut re = vec![0.0; 8];
        let mut im = vec![0.0; 8];
        re[0] = 1.0;
        fft(&mut re, &mut im);
        assert!(re.iter().all(|&x| (x - 1.0).abs() < 1e-12));
        assert!(im.iter().all(|&x| x.abs() < 1e-12));
    }

    #[test]
    fn matches_naive_dft() {
        let n = 16;
        let x: Vec<f64> = (0..n).map(|i| ((i * i) % 7) as f64 - 3.0).collect();
        let mut re = x.clone();
        let mut im = vec![0.0; n];
        fft(&mut re, &mut im);
        for k in 0..n {
            let (mut sr, mut si) = (0.0, 0.0);
            for (t, v) in x.iter().enumerate() {
                let a = -2.0 * PI * (k * t) as f64 / n as f64;
                sr += v * a.cos();
                si += v * a.sin();
            }
            assert!((re[k] - sr).abs() < 1e-9, "re {k}");
            assert!((im[k] - si).abs() < 1e-9, "im {k}");
        }
    }

    #[test]
    fn sine_peaks_at_its_frequency_with_its_amplitude() {
        let rate = 8192.0;
        let volts = (0..4096)
            .map(|i| (1.5 * (2.0 * PI * 512.0 * i as f64 / rate).sin()) as f32)
            .collect();
        let w = Waveform { t0: 0.0, dt: 1.0 / rate, volts, vdiv: 1.0, tdiv: 1.0 };
        let s = spectrum(&w).unwrap();
        let (bin, db) = s.peak().unwrap();
        assert!((s.frequency(bin) - 512.0).abs() < s.df);
        let expect = 20.0 * 1.5f32.log10();
        assert!((db - expect).abs() < 0.5, "{db} vs {expect}");
    }

    #[test]
    fn uses_largest_power_of_two() {
        let w = Waveform { t0: 0.0, dt: 1e-3, volts: vec![0.0; 1000], vdiv: 1.0, tdiv: 1.0 };
        let s = spectrum(&w).unwrap();
        assert_eq!(s.db.len(), 512 / 2 + 1);
        assert!((s.df - 1000.0 / 512.0).abs() < 1e-9);
    }

    #[test]
    fn short_input_is_none() {
        let w = Waveform { t0: 0.0, dt: 1e-3, volts: vec![0.0; 2], vdiv: 1.0, tdiv: 1.0 };
        assert!(spectrum(&w).is_none());
    }

    #[test]
    fn hann_ends_at_zero() {
        let h = hann(9);
        assert!(h[0].abs() < 1e-12 && h[8].abs() < 1e-12);
        assert!((h[4] - 1.0).abs() < 1e-12);
    }
}
