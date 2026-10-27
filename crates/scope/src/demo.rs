use crate::scope::{Description, Scope, Slope, Trigger};
use crate::waveform::Waveform;
use anyhow::{Result, bail};
use std::f64::consts::PI;

pub const POINTS: usize = 1200;
const DIVS: f64 = 12.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    Sine,
    Noisy,
    Square,
    Triangle,
}

pub struct DemoScope {
    desc: Description,
    tdiv: f64,
    vdiv: [f64; 4],
    offset: [f64; 4],
    pub trigger: Trigger,
    running: bool,
    phase: f64,
    seed: u64,
}

impl Default for DemoScope {
    fn default() -> Self {
        Self::new()
    }
}

impl DemoScope {
    pub fn new() -> Self {
        Self {
            desc: Description {
                manufacturer: "usbscope".into(),
                model: "DEMO4".into(),
                serial: "0001".into(),
                firmware: "1.0".into(),
                channels: 4,
            },
            tdiv: 1e-3,
            vdiv: [1.0; 4],
            offset: [0.0; 4],
            trigger: Trigger { channel: 0, level: 0.0, slope: Slope::Rising },
            running: true,
            phase: 0.0,
            seed: 0x2545_F491_4F6C_DD1D,
        }
    }

    pub fn shape(channel: usize) -> Shape {
        [Shape::Sine, Shape::Noisy, Shape::Square, Shape::Triangle][channel % 4]
    }

    pub fn running(&self) -> bool {
        self.running
    }

    fn noise(&mut self) -> f64 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        (self.seed >> 11) as f64 / (1u64 << 53) as f64 - 0.5
    }

    pub fn sample(shape: Shape, cycles: f64) -> f64 {
        let frac = cycles - cycles.floor();
        match shape {
            Shape::Sine | Shape::Noisy => (2.0 * PI * cycles).sin(),
            Shape::Square => {
                if frac < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            Shape::Triangle => 4.0 * (frac - 0.5).abs() - 1.0,
        }
    }
}

impl Scope for DemoScope {
    fn describe(&self) -> &Description {
        &self.desc
    }

    fn set_timebase(&mut self, tdiv: f64) -> Result<()> {
        if tdiv <= 0.0 {
            bail!("time per division must be positive");
        }
        self.tdiv = tdiv;
        Ok(())
    }

    fn set_vertical(&mut self, channel: usize, vdiv: f64, offset: f64) -> Result<()> {
        if channel >= 4 || vdiv <= 0.0 {
            bail!("bad channel or scale");
        }
        self.vdiv[channel] = vdiv;
        self.offset[channel] = offset;
        Ok(())
    }

    fn set_trigger(&mut self, trigger: Trigger) -> Result<()> {
        self.trigger = trigger;
        Ok(())
    }

    fn run(&mut self) -> Result<()> {
        self.running = true;
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        self.running = false;
        Ok(())
    }

    fn single(&mut self) -> Result<()> {
        self.running = false;
        self.phase += 0.137;
        Ok(())
    }

    fn capture(&mut self, channel: usize) -> Result<Waveform> {
        if channel >= 4 {
            bail!("channel {} out of range", channel + 1);
        }
        if self.running {
            self.phase += 0.031;
        }
        let span = self.tdiv * DIVS;
        let dt = span / POINTS as f64;
        let base = 1000.0 * (channel + 1) as f64;
        let amp = 2.0 * self.vdiv[channel];
        let shape = Self::shape(channel);
        let noisy = shape == Shape::Noisy;
        let volts = (0..POINTS)
            .map(|i| {
                let t = i as f64 * dt - span / 2.0;
                let cycles = base * t + self.phase.fract();
                let n = if noisy { self.noise() * 0.3 * self.vdiv[channel] } else { 0.0 };
                (amp * Self::sample(shape, cycles) + n + self.offset[channel]) as f32
            })
            .collect();
        Ok(Waveform { t0: -span / 2.0, dt, volts, vdiv: self.vdiv[channel], tdiv: self.tdiv })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measure::measure;

    #[test]
    fn four_channels_with_distinct_shapes() {
        let mut s = DemoScope::new();
        assert_eq!(s.channels(), 4);
        let a = s.capture(0).unwrap();
        let b = s.capture(2).unwrap();
        assert_eq!(a.len(), POINTS);
        assert_ne!(a.volts, b.volts);
        assert!(s.capture(4).is_err());
    }

    #[test]
    fn sine_measures_at_its_frequency() {
        let mut s = DemoScope::new();
        let m = measure(&s.capture(0).unwrap()).unwrap();
        assert!((m.frequency.unwrap() - 1000.0).abs() < 20.0);
        assert!((m.vpp - 4.0).abs() < 0.1);
    }

    #[test]
    fn square_has_half_duty() {
        let mut s = DemoScope::new();
        s.set_timebase(5e-4).unwrap();
        let m = measure(&s.capture(2).unwrap()).unwrap();
        assert!((m.frequency.unwrap() - 3000.0).abs() < 60.0);
        assert!((m.duty.unwrap() - 0.5).abs() < 0.03);
    }

    #[test]
    fn stop_freezes_and_scale_applies() {
        let mut s = DemoScope::new();
        s.stop().unwrap();
        let a = s.capture(1).unwrap();
        let b = s.capture(1).unwrap();
        assert_eq!(a.volts.len(), b.volts.len());
        s.set_vertical(0, 2.0, 1.0).unwrap();
        let m = measure(&s.capture(0).unwrap()).unwrap();
        assert!((m.mean - 1.0).abs() < 0.1);
        assert!((m.vpp - 8.0).abs() < 0.2);
        assert!(s.set_vertical(0, 0.0, 0.0).is_err());
        assert!(s.set_timebase(0.0).is_err());
    }

    #[test]
    fn triangle_range() {
        assert!((DemoScope::sample(Shape::Triangle, 0.0) - 1.0).abs() < 1e-9);
        assert!((DemoScope::sample(Shape::Triangle, 0.5) + 1.0).abs() < 1e-9);
    }
}
