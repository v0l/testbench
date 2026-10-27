use std::collections::VecDeque;

#[derive(Debug, Clone, Default)]
pub struct Series {
    data: VecDeque<(f64, f64)>,
    cap: usize,
}

impl Series {
    pub fn new(cap: usize) -> Self {
        Self { data: VecDeque::new(), cap: cap.max(1) }
    }

    pub fn push(&mut self, t: f64, v: f64) {
        if self.data.len() == self.cap {
            self.data.pop_front();
        }
        self.data.push_back((t, v));
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn clear(&mut self) {
        self.data.clear();
    }

    pub fn last(&self) -> Option<(f64, f64)> {
        self.data.back().copied()
    }

    pub fn window(&self, from: f64) -> Vec<(f64, f64)> {
        self.data.iter().copied().filter(|(t, _)| *t >= from).collect()
    }

    pub fn values(&self) -> Vec<f32> {
        self.data.iter().map(|(_, v)| *v as f32).collect()
    }

    pub fn span(&self) -> Option<(f64, f64)> {
        Some((self.data.front()?.0, self.data.back()?.0))
    }
}

pub fn value_range(series: &[&[(f64, f64)]], floor_span: f64) -> (f64, f64) {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for s in series {
        for (_, v) in s.iter() {
            lo = lo.min(*v);
            hi = hi.max(*v);
        }
    }
    if !lo.is_finite() {
        return (0.0, floor_span.max(1.0));
    }
    lo = lo.min(0.0);
    let span = (hi - lo).max(floor_span);
    (lo, lo + span * 1.1)
}

pub fn time_range(series: &[&[(f64, f64)]], span: f64) -> (f64, f64) {
    let end = series
        .iter()
        .filter_map(|s| s.last())
        .map(|(t, _)| *t)
        .fold(f64::NEG_INFINITY, f64::max);
    if !end.is_finite() {
        return (0.0, span);
    }
    (end - span, end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn series_drops_the_oldest_past_capacity() {
        let mut s = Series::new(3);
        for i in 0..5 {
            s.push(i as f64, i as f64 * 10.0);
        }
        assert_eq!(s.len(), 3);
        assert_eq!(s.span(), Some((2.0, 4.0)));
        assert_eq!(s.last(), Some((4.0, 40.0)));
    }

    #[test]
    fn window_keeps_only_recent_samples() {
        let mut s = Series::new(10);
        for i in 0..6 {
            s.push(i as f64, 1.0);
        }
        assert_eq!(s.window(4.0).len(), 2);
    }

    #[test]
    fn ranges_cover_both_series_on_one_axis() {
        let a = [(0.0, 1.0), (1.0, 5.0)];
        let b = [(0.5, 2.0), (3.0, 9.0)];
        let (lo, hi) = value_range(&[&a, &b], 1.0);
        assert_eq!(lo, 0.0);
        assert!(hi > 9.0);
        assert_eq!(time_range(&[&a, &b], 10.0), (-7.0, 3.0));
    }

    #[test]
    fn empty_series_give_a_usable_default_range() {
        assert_eq!(value_range(&[], 2.0), (0.0, 2.0));
        assert_eq!(time_range(&[], 60.0), (0.0, 60.0));
    }
}
