#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SweepParams {
    pub start_hz: f64,
    pub stop_hz: f64,
    pub points: usize,
}

pub const MAX_POINTS: usize = 1001;
pub const MIN_SPAN_HZ: f64 = 1000.0;

impl SweepParams {
    pub fn fit(self, range: (f64, f64)) -> SweepParams {
        let (lo, hi) = range;
        let (mut a, mut b) = (self.start_hz, self.stop_hz);
        if !a.is_finite() {
            a = lo;
        }
        if !b.is_finite() {
            b = hi;
        }
        if a > b {
            std::mem::swap(&mut a, &mut b);
        }
        a = a.clamp(lo, hi);
        b = b.clamp(lo, hi);
        if b - a < MIN_SPAN_HZ {
            if a + MIN_SPAN_HZ <= hi {
                b = a + MIN_SPAN_HZ;
            } else {
                b = hi;
                a = hi - MIN_SPAN_HZ;
            }
        }
        SweepParams { start_hz: a, stop_hz: b, points: self.points.clamp(2, MAX_POINTS) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RANGE: (f64, f64) = (50e3, 900e6);

    fn p(a: f64, b: f64, n: usize) -> SweepParams {
        SweepParams { start_hz: a, stop_hz: b, points: n }
    }

    #[test]
    fn reversed_edges_are_swapped() {
        assert_eq!(p(20e6, 10e6, 101).fit(RANGE), p(10e6, 20e6, 101));
    }

    #[test]
    fn edges_are_pulled_into_the_device_range() {
        let f = p(0.0, 5e9, 101).fit(RANGE);
        assert_eq!((f.start_hz, f.stop_hz), RANGE);
    }

    #[test]
    fn a_zero_span_gets_a_minimum_width() {
        let f = p(7e6, 7e6, 101).fit(RANGE);
        assert_eq!(f.stop_hz - f.start_hz, MIN_SPAN_HZ);
        let top = p(900e6, 900e6, 101).fit(RANGE);
        assert_eq!(top.stop_hz, 900e6);
        assert_eq!(top.stop_hz - top.start_hz, MIN_SPAN_HZ);
    }

    #[test]
    fn point_count_is_bounded() {
        assert_eq!(p(1e6, 2e6, 0).fit(RANGE).points, 2);
        assert_eq!(p(1e6, 2e6, 99999).fit(RANGE).points, MAX_POINTS);
    }
}
