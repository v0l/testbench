use testbench_scope::Waveform;

pub const DIVS_X: usize = 10;
pub const DIVS_Y: usize = 8;

pub fn volts_to_div(v: f64, vdiv: f64, offset: f64) -> f64 {
    if vdiv <= 0.0 { 0.0 } else { (v + offset) / vdiv }
}

pub fn to_unit(x_div: f64, y_div: f64) -> (f64, f64) {
    (
        x_div / DIVS_X as f64 + 0.5,
        0.5 - y_div / DIVS_Y as f64,
    )
}

pub fn trace_points(w: &Waveform, vdiv: f64, offset: f64, max_points: usize) -> Vec<(f64, f64)> {
    if w.volts.is_empty() || w.tdiv <= 0.0 || max_points < 2 {
        return Vec::new();
    }
    let step = w.volts.len().div_ceil(max_points).max(1);
    let mut out = Vec::with_capacity(w.volts.len() / step + 1);
    let mut i = 0;
    while i < w.volts.len() {
        let end = (i + step).min(w.volts.len());
        let chunk = &w.volts[i..end];
        let (lo, hi) = chunk
            .iter()
            .fold((f32::MAX, f32::MIN), |(l, h), &v| (l.min(v), h.max(v)));
        let t = (w.time(i) + w.time(end - 1)) / 2.0;
        let x = t / w.tdiv;
        let (a, b) = if step == 1 { (hi, hi) } else { (lo, hi) };
        out.push(to_unit(x, volts_to_div(f64::from(a), vdiv, offset)));
        if a != b {
            out.push(to_unit(x, volts_to_div(f64::from(b), vdiv, offset)));
        }
        i = end;
    }
    out
}

pub fn si(value: f64, unit: &str) -> String {
    let a = value.abs();
    let (scale, prefix) = if a == 0.0 {
        (1.0, "")
    } else if a >= 1e9 {
        (1e-9, "G")
    } else if a >= 1e6 {
        (1e-6, "M")
    } else if a >= 1e3 {
        (1e-3, "k")
    } else if a >= 1.0 {
        (1.0, "")
    } else if a >= 1e-3 {
        (1e3, "m")
    } else if a >= 1e-6 {
        (1e6, "u")
    } else {
        (1e9, "n")
    };
    format!("{:.3} {prefix}{unit}", value * scale)
}

pub const TDIV_STEPS: [f64; 19] = [
    1e-6, 2e-6, 5e-6, 1e-5, 2e-5, 5e-5, 1e-4, 2e-4, 5e-4, 1e-3, 2e-3, 5e-3, 1e-2, 2e-2, 5e-2, 0.1,
    0.2, 0.5, 1.0,
];

pub const VDIV_STEPS: [f64; 10] = [0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0];

pub fn step_index(steps: &[f64], value: f64) -> usize {
    steps
        .iter()
        .enumerate()
        .min_by(|a, b| (a.1 - value).abs().total_cmp(&(b.1 - value).abs()))
        .map(|(i, _)| i)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wave(volts: Vec<f32>) -> Waveform {
        Waveform { t0: -5e-3, dt: 1e-5, volts, vdiv: 1.0, tdiv: 1e-3 }
    }

    #[test]
    fn centre_of_the_screen_is_the_middle_of_the_unit_square() {
        assert_eq!(to_unit(0.0, 0.0), (0.5, 0.5));
        assert_eq!(to_unit(5.0, 4.0), (1.0, 0.0));
        assert_eq!(to_unit(-5.0, -4.0), (0.0, 1.0));
    }

    #[test]
    fn volts_scale_by_division_and_offset() {
        assert_eq!(volts_to_div(2.0, 0.5, 0.0), 4.0);
        assert_eq!(volts_to_div(1.0, 1.0, 1.0), 2.0);
        assert_eq!(volts_to_div(1.0, 0.0, 0.0), 0.0);
    }

    #[test]
    fn full_resolution_trace_has_one_point_per_sample() {
        let w = wave((0..100).map(|i| i as f32 * 0.01).collect());
        let pts = trace_points(&w, 1.0, 0.0, 1000);
        assert_eq!(pts.len(), 100);
        assert!(pts[0].0 >= 0.0 && pts[99].0 <= 1.0);
    }

    #[test]
    fn decimated_trace_keeps_the_extremes_of_each_bucket() {
        let mut v = vec![0.0f32; 1000];
        v[503] = 3.0;
        let w = wave(v);
        let pts = trace_points(&w, 1.0, 0.0, 100);
        assert!(pts.len() <= 2 * 100);
        let top = pts.iter().map(|p| p.1).fold(f64::MAX, f64::min);
        assert!((top - (0.5 - 3.0 / 8.0)).abs() < 1e-9);
    }

    #[test]
    fn degenerate_waveforms_give_no_trace() {
        assert!(trace_points(&wave(Vec::new()), 1.0, 0.0, 100).is_empty());
        assert!(trace_points(&wave(vec![1.0]), 1.0, 0.0, 1).is_empty());
    }

    #[test]
    fn si_picks_the_prefix() {
        assert_eq!(si(0.0015, "s"), "1.500 ms");
        assert_eq!(si(2500.0, "Hz"), "2.500 kHz");
        assert_eq!(si(0.0, "V"), "0.000 V");
        assert_eq!(si(-4e-7, "s"), "-400.000 ns");
    }

    #[test]
    fn step_index_snaps_to_the_nearest_step() {
        assert_eq!(VDIV_STEPS[step_index(&VDIV_STEPS, 0.45)], 0.5);
        assert_eq!(TDIV_STEPS[step_index(&TDIV_STEPS, 1.1e-3)], 1e-3);
    }
}
