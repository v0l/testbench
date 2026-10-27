use antenna_vna::Point;

pub fn nice_ticks(lo: f64, hi: f64, target: usize) -> Vec<f64> {
    if hi.partial_cmp(&lo) != Some(std::cmp::Ordering::Greater) || !lo.is_finite() || !hi.is_finite() || target == 0 {
        return Vec::new();
    }
    let raw = (hi - lo) / target as f64;
    let mag = 10f64.powf(raw.log10().floor());
    let step = [1.0, 2.0, 5.0, 10.0]
        .iter()
        .map(|m| m * mag)
        .find(|s| *s >= raw)
        .unwrap_or(10.0 * mag);
    let mut out = Vec::new();
    let mut k = (lo / step).ceil();
    while k * step <= hi + step * 1e-9 {
        out.push((k * step * 1e9).round() / 1e9);
        k += 1.0;
    }
    out
}

pub fn format_freq(hz: f64) -> String {
    let a = hz.abs();
    if a >= 1e9 {
        format!("{:.4} GHz", hz / 1e9)
    } else if a >= 1e6 {
        format!("{:.4} MHz", hz / 1e6)
    } else if a >= 1e3 {
        format!("{:.3} kHz", hz / 1e3)
    } else {
        format!("{hz:.0} Hz")
    }
}

pub fn nearest_index(freqs: &[f64], target: f64) -> Option<usize> {
    freqs
        .iter()
        .enumerate()
        .min_by(|a, b| (a.1 - target).abs().total_cmp(&(b.1 - target).abs()))
        .map(|(i, _)| i)
}

pub fn best_match(points: &[Point]) -> Option<usize> {
    points
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.swr().total_cmp(&b.1.swr()))
        .map(|(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;
    use antenna_vna::C64;

    #[test]
    fn ticks_land_on_round_numbers_inside_the_range() {
        assert_eq!(nice_ticks(0.0, 10.0, 5), vec![0.0, 2.0, 4.0, 6.0, 8.0, 10.0]);
        assert_eq!(nice_ticks(1.0, 3.0, 4), vec![1.0, 1.5, 2.0, 2.5, 3.0]);
        assert!(nice_ticks(5.0, 5.0, 4).is_empty());
    }

    #[test]
    fn frequencies_read_in_the_nearest_unit() {
        assert_eq!(format_freq(14.2e6), "14.2000 MHz");
        assert_eq!(format_freq(500.0), "500 Hz");
        assert_eq!(format_freq(2.4e9), "2.4000 GHz");
    }

    #[test]
    fn nearest_index_picks_the_closest_frequency() {
        let f = [1.0, 2.0, 3.0];
        assert_eq!(nearest_index(&f, 2.4), Some(1));
        assert_eq!(nearest_index(&f, 99.0), Some(2));
        assert_eq!(nearest_index(&[], 1.0), None);
    }

    #[test]
    fn best_match_is_the_lowest_swr() {
        let p = |f, re| Point { freq: f, s11: C64::new(re, 0.0), s21: None };
        assert_eq!(best_match(&[p(1.0, 0.5), p(2.0, 0.1), p(3.0, 0.3)]), Some(1));
    }
}
