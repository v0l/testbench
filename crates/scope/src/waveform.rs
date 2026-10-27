#[derive(Debug, Clone, PartialEq)]
pub struct Waveform {
    pub t0: f64,
    pub dt: f64,
    pub volts: Vec<f32>,
    pub vdiv: f64,
    pub tdiv: f64,
}

impl Waveform {
    pub fn len(&self) -> usize {
        self.volts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.volts.is_empty()
    }

    pub fn time(&self, i: usize) -> f64 {
        self.t0 + self.dt * i as f64
    }

    pub fn duration(&self) -> f64 {
        self.dt * self.volts.len().saturating_sub(1) as f64
    }

    pub fn at(&self, t: f64) -> Option<f32> {
        if self.volts.is_empty() || self.dt <= 0.0 {
            return None;
        }
        let x = (t - self.t0) / self.dt;
        if x < 0.0 || x > (self.volts.len() - 1) as f64 {
            return None;
        }
        let i = x.floor() as usize;
        let f = (x - i as f64) as f32;
        let a = self.volts[i];
        let b = self.volts.get(i + 1).copied().unwrap_or(a);
        Some(a + (b - a) * f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp() -> Waveform {
        Waveform { t0: -1.0, dt: 0.5, volts: vec![0.0, 1.0, 2.0, 3.0], vdiv: 1.0, tdiv: 1.0 }
    }

    #[test]
    fn interpolates_between_samples() {
        let w = ramp();
        assert_eq!(w.at(-1.0), Some(0.0));
        assert_eq!(w.at(-0.75), Some(0.5));
        assert_eq!(w.at(0.5), Some(3.0));
        assert_eq!(w.at(0.6), None);
        assert_eq!(w.at(-1.1), None);
    }

    #[test]
    fn time_and_duration() {
        let w = ramp();
        assert_eq!(w.time(2), 0.0);
        assert_eq!(w.duration(), 1.5);
    }

    #[test]
    fn empty_has_no_value() {
        let w = Waveform { t0: 0.0, dt: 1.0, volts: vec![], vdiv: 1.0, tdiv: 1.0 };
        assert_eq!(w.at(0.0), None);
    }
}
