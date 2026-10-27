use crate::waveform::Waveform;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measurements {
    pub vpp: f64,
    pub vmax: f64,
    pub vmin: f64,
    pub vrms: f64,
    pub mean: f64,
    pub frequency: Option<f64>,
    pub period: Option<f64>,
    pub rise_time: Option<f64>,
    pub duty: Option<f64>,
}

pub fn measure(w: &Waveform) -> Option<Measurements> {
    if w.volts.len() < 2 || w.dt <= 0.0 {
        return None;
    }
    let n = w.volts.len() as f64;
    let vmax = w.volts.iter().copied().fold(f32::MIN, f32::max) as f64;
    let vmin = w.volts.iter().copied().fold(f32::MAX, f32::min) as f64;
    let mean = w.volts.iter().map(|&v| f64::from(v)).sum::<f64>() / n;
    let vrms = (w.volts.iter().map(|&v| f64::from(v).powi(2)).sum::<f64>() / n).sqrt();
    let vpp = vmax - vmin;
    let mut m = Measurements {
        vpp,
        vmax,
        vmin,
        vrms,
        mean,
        frequency: None,
        period: None,
        rise_time: None,
        duty: None,
    };
    if vpp <= 0.0 {
        return Some(m);
    }
    let mid = (vmax + vmin) / 2.0;
    let hyst = vpp * 0.05;
    let rising = crossings(&w.volts, mid, hyst, true);
    let falling = crossings(&w.volts, mid, hyst, false);
    if rising.len() >= 2 {
        let span = rising[rising.len() - 1] - rising[0];
        let period = span / (rising.len() - 1) as f64 * w.dt;
        if period > 0.0 {
            m.period = Some(period);
            m.frequency = Some(1.0 / period);
        }
        if let Some(duty) = duty(&rising, &falling) {
            m.duty = Some(duty);
        }
    }
    m.rise_time = rise_time(&w.volts, vmin, vmax).map(|s| s * w.dt);
    Some(m)
}

pub fn crossings(volts: &[f32], level: f64, hyst: f64, rising: bool) -> Vec<f64> {
    let mut out = Vec::new();
    let (lo, hi) = (level - hyst, level + hyst);
    let mut armed = false;
    for i in 0..volts.len() {
        let v = f64::from(volts[i]);
        if rising {
            if v < lo {
                armed = true;
            }
            if armed && v >= level && i > 0 {
                out.push(interpolate(volts, i, level));
                armed = false;
            }
        } else {
            if v > hi {
                armed = true;
            }
            if armed && v <= level && i > 0 {
                out.push(interpolate(volts, i, level));
                armed = false;
            }
        }
    }
    out
}

fn interpolate(volts: &[f32], i: usize, level: f64) -> f64 {
    let a = f64::from(volts[i - 1]);
    let b = f64::from(volts[i]);
    if (b - a).abs() < f64::EPSILON {
        return i as f64;
    }
    (i - 1) as f64 + (level - a) / (b - a)
}

fn duty(rising: &[f64], falling: &[f64]) -> Option<f64> {
    let start = *rising.first()?;
    let end = *rising.get(1)?;
    let fall = falling.iter().copied().find(|&f| f > start && f < end)?;
    Some((fall - start) / (end - start))
}

fn rise_time(volts: &[f32], vmin: f64, vmax: f64) -> Option<f64> {
    let span = vmax - vmin;
    let (lo, hi) = (vmin + 0.1 * span, vmin + 0.9 * span);
    let mut state = 0;
    let mut t_lo = 0.0;
    for i in 1..volts.len() {
        let a = f64::from(volts[i - 1]);
        let b = f64::from(volts[i]);
        if b <= a {
            state = 0;
            continue;
        }
        if state == 0 && a < lo && b >= lo {
            t_lo = interpolate(volts, i, lo);
            state = 1;
        }
        if state == 1 && b >= hi {
            return Some(interpolate(volts, i, hi) - t_lo);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    fn sine(freq: f64, amp: f64, offset: f64, rate: f64, n: usize) -> Waveform {
        let dt = 1.0 / rate;
        let volts = (0..n)
            .map(|i| (offset + amp * (2.0 * PI * freq * i as f64 * dt).sin()) as f32)
            .collect();
        Waveform { t0: 0.0, dt, volts, vdiv: 1.0, tdiv: 1.0 }
    }

    fn square(period: usize, high: usize, cycles: usize, lo: f32, hi: f32) -> Waveform {
        let mut volts = Vec::new();
        for _ in 0..cycles {
            for i in 0..period {
                volts.push(if i < high { hi } else { lo });
            }
        }
        Waveform { t0: 0.0, dt: 1e-6, volts, vdiv: 1.0, tdiv: 1.0 }
    }

    #[test]
    fn sine_amplitude_rms_mean() {
        let m = measure(&sine(1000.0, 2.0, 0.5, 100_000.0, 10_000)).unwrap();
        assert!((m.vpp - 4.0).abs() < 1e-3);
        assert!((m.mean - 0.5).abs() < 1e-3);
        assert!((m.vrms - (0.25f64 + 2.0).sqrt()).abs() < 1e-3);
        assert!((m.vmax - 2.5).abs() < 1e-3);
        assert!((m.vmin + 1.5).abs() < 1e-3);
    }

    #[test]
    fn sine_frequency_and_period() {
        let m = measure(&sine(1234.0, 1.0, 0.0, 200_000.0, 20_000)).unwrap();
        assert!((m.frequency.unwrap() - 1234.0).abs() < 1.0);
        assert!((m.period.unwrap() - 1.0 / 1234.0).abs() < 1e-6);
    }

    #[test]
    fn square_duty_and_frequency() {
        let m = measure(&square(100, 30, 8, 0.0, 3.0)).unwrap();
        assert!((m.duty.unwrap() - 0.3).abs() < 0.02);
        assert!((m.frequency.unwrap() - 10_000.0).abs() < 50.0);
        assert!((m.vpp - 3.0).abs() < 1e-6);
    }

    #[test]
    fn rise_time_of_a_linear_edge() {
        let mut volts = vec![0.0f32; 20];
        for i in 0..10 {
            volts.push(i as f32 / 10.0);
        }
        volts.extend(vec![1.0f32; 20]);
        let w = Waveform { t0: 0.0, dt: 1e-9, volts, vdiv: 1.0, tdiv: 1.0 };
        let m = measure(&w).unwrap();
        let rt = m.rise_time.unwrap();
        assert!((rt - 8.0e-9).abs() < 0.3e-9, "{rt}");
    }

    #[test]
    fn dc_has_no_frequency() {
        let w = Waveform { t0: 0.0, dt: 1e-6, volts: vec![1.5; 100], vdiv: 1.0, tdiv: 1.0 };
        let m = measure(&w).unwrap();
        assert_eq!(m.vpp, 0.0);
        assert_eq!(m.frequency, None);
        assert_eq!(m.rise_time, None);
        assert!((m.vrms - 1.5).abs() < 1e-9);
    }

    #[test]
    fn too_short_is_none() {
        let w = Waveform { t0: 0.0, dt: 1e-6, volts: vec![1.0], vdiv: 1.0, tdiv: 1.0 };
        assert!(measure(&w).is_none());
    }

    #[test]
    fn noise_inside_hysteresis_does_not_double_count() {
        let mut volts = Vec::new();
        for c in 0..5 {
            let _ = c;
            for i in 0..50 {
                let jitter = if i % 2 == 0 { 0.01 } else { -0.01 };
                let base = if i < 25 { 0.0 } else { 1.0 };
                volts.push(base as f32 + jitter);
            }
        }
        let w = Waveform { t0: 0.0, dt: 1e-6, volts, vdiv: 1.0, tdiv: 1.0 };
        let m = measure(&w).unwrap();
        assert!((m.frequency.unwrap() - 20_000.0).abs() < 200.0);
    }
}
