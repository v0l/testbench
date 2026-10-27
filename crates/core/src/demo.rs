use anyhow::Result;
use antenna_vna::{C64, Point, Vna};
use cycler_core::device::{Charger, Device, Discharger, Limits, LoadMode, LoadState, Regulation, Sample};
use std::time::Instant;

pub const SUPPLY_OHMS: f64 = 8.0;
pub const LOAD_SOURCE_V: f64 = 12.0;
pub const LOAD_SOURCE_OHMS: f64 = 0.3;

pub fn supply_solve(set_v: f64, set_a: f64, on: bool, ohms: f64) -> (f64, f64, Regulation) {
    if !on {
        return (0.0, 0.0, Regulation::Standby);
    }
    let a_cv = set_v / ohms;
    if a_cv <= set_a {
        (set_v, a_cv, Regulation::Cv)
    } else {
        (set_a * ohms, set_a, Regulation::Cc)
    }
}

pub struct DemoSupply {
    set_v: f64,
    set_a: f64,
    on: bool,
    armed: Option<(f64, f64)>,
    born: Instant,
}

impl DemoSupply {
    pub fn new() -> Self {
        Self { set_v: 0.0, set_a: 0.0, on: false, armed: None, born: Instant::now() }
    }

    pub fn left_on() -> Self {
        Self { set_v: 5.0, set_a: 1.0, on: true, ..Self::new() }
    }

    pub fn armed(&self) -> Option<(f64, f64)> {
        self.armed
    }

    fn ceiling(&self) -> (f64, f64) {
        let (v, a): (f64, f64) = (30.0, 5.0);
        match self.armed {
            Some((av, aa)) => (v.min(av), a.min(aa)),
            None => (v, a),
        }
    }
}

impl Default for DemoSupply {
    fn default() -> Self {
        Self::new()
    }
}

impl Device for DemoSupply {
    fn name(&self) -> String {
        "Demo PSU 30 V 5 A".into()
    }

    fn limits(&self) -> Limits {
        Limits { max_volts: 30.0, max_amps: 5.0, max_watts: 150.0 }
    }

    fn measure(&mut self) -> Result<Sample> {
        let (v, a, _) = supply_solve(self.set_v, self.set_a, self.on, SUPPLY_OHMS);
        let ripple = 1.0 + 0.002 * (self.born.elapsed().as_secs_f64() * 3.0).sin();
        Ok(Sample { volts: v * ripple, amps: a * ripple })
    }

    fn stop(&mut self) -> Result<()> {
        self.on = false;
        Ok(())
    }

    fn output_on(&mut self) -> Result<Option<bool>> {
        Ok(Some(self.on))
    }
}

impl Charger for DemoSupply {
    fn regulation(&mut self) -> Result<Option<Regulation>> {
        Ok(Some(supply_solve(self.set_v, self.set_a, self.on, SUPPLY_OHMS).2))
    }

    fn arm(&mut self, max_volts: f64, max_amps: f64) -> Result<bool> {
        self.armed = Some((max_volts, max_amps));
        self.set_v = self.set_v.min(max_volts);
        self.set_a = self.set_a.min(max_amps);
        Ok(true)
    }

    fn set(&mut self, volts: f64, amps: f64) -> Result<()> {
        let (mv, ma) = self.ceiling();
        self.set_v = volts.clamp(0.0, mv);
        self.set_a = amps.clamp(0.0, ma);
        Ok(())
    }

    fn start(&mut self) -> Result<()> {
        self.on = true;
        Ok(())
    }
}

pub fn load_solve(mode: LoadMode, value: f64, source_v: f64, source_ohms: f64) -> (f64, f64) {
    let r = source_ohms;
    let amps = match mode {
        LoadMode::Cc => value.clamp(0.0, source_v / r),
        LoadMode::Cr => source_v / (value.max(1e-6) + r),
        LoadMode::Cv => {
            if value >= source_v {
                0.0
            } else {
                (source_v - value.max(0.0)) / r
            }
        }
        LoadMode::Cp => {
            let disc = source_v * source_v - 4.0 * r * value.max(0.0);
            if disc < 0.0 {
                source_v / (2.0 * r)
            } else {
                (source_v - disc.sqrt()) / (2.0 * r)
            }
        }
    };
    ((source_v - amps * r).max(0.0), amps)
}

pub struct DemoLoad {
    mode: LoadMode,
    value: f64,
    on: bool,
    amp_hours: f64,
    watt_hours: f64,
    runtime_s: f64,
    last: Instant,
}

impl DemoLoad {
    pub fn new() -> Self {
        Self {
            mode: LoadMode::Cc,
            value: 0.0,
            on: false,
            amp_hours: 0.0,
            watt_hours: 0.0,
            runtime_s: 0.0,
            last: Instant::now(),
        }
    }

    pub fn left_on() -> Self {
        Self { value: 1.0, on: true, ..Self::new() }
    }

    pub fn integrate(&mut self, dt_s: f64) {
        if !self.on || dt_s <= 0.0 {
            return;
        }
        let (v, a) = load_solve(self.mode, self.value, LOAD_SOURCE_V, LOAD_SOURCE_OHMS);
        self.amp_hours += a * dt_s / 3600.0;
        self.watt_hours += v * a * dt_s / 3600.0;
        self.runtime_s += dt_s;
    }

    fn tick(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last).as_secs_f64();
        self.last = now;
        self.integrate(dt);
    }

    fn snapshot(&self) -> LoadState {
        let (v, a) = if self.on {
            load_solve(self.mode, self.value, LOAD_SOURCE_V, LOAD_SOURCE_OHMS)
        } else {
            (LOAD_SOURCE_V, 0.0)
        };
        let w = v * a;
        LoadState {
            setpoint: self.value,
            volts: v,
            amps: a,
            watts: w,
            ohms: if a > 1e-6 { Some(v / a) } else { None },
            amp_hours: self.amp_hours,
            watt_hours: self.watt_hours,
            temp_c: 25.0 + 0.35 * w,
            runtime_s: self.runtime_s,
            on: self.on,
        }
    }
}

impl Default for DemoLoad {
    fn default() -> Self {
        Self::new()
    }
}

impl Device for DemoLoad {
    fn name(&self) -> String {
        "Demo load 150 W".into()
    }

    fn limits(&self) -> Limits {
        Limits { max_volts: 30.0, max_amps: 10.0, max_watts: 150.0 }
    }

    fn measure(&mut self) -> Result<Sample> {
        self.tick();
        let s = self.snapshot();
        Ok(Sample { volts: s.volts, amps: s.amps })
    }

    fn stop(&mut self) -> Result<()> {
        self.tick();
        self.on = false;
        Ok(())
    }

    fn output_on(&mut self) -> Result<Option<bool>> {
        Ok(Some(self.on))
    }
}

impl Discharger for DemoLoad {
    fn modes(&self) -> &'static [LoadMode] {
        &[LoadMode::Cc, LoadMode::Cp, LoadMode::Cv, LoadMode::Cr]
    }

    fn set_mode(&mut self, mode: LoadMode, value: f64) -> Result<()> {
        self.tick();
        self.mode = mode;
        self.value = value;
        Ok(())
    }

    fn set_current(&mut self, amps: f64) -> Result<()> {
        self.set_mode(LoadMode::Cc, amps)
    }

    fn start(&mut self) -> Result<()> {
        self.tick();
        self.on = true;
        Ok(())
    }

    fn as_any(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn state(&mut self) -> Result<LoadState> {
        self.tick();
        Ok(self.snapshot())
    }
}

pub const ANTENNA_F0: f64 = 14.2e6;

pub fn antenna_gamma(freq: f64) -> C64 {
    let r = 38.0 + 6.0 * ((freq - ANTENNA_F0) / 1.5e6).powi(2);
    let x = 140.0 * (freq / ANTENNA_F0 - ANTENNA_F0 / freq);
    let z = C64::new(r, x);
    let z0 = C64::new(50.0, 0.0);
    (z - z0) / (z + z0)
}

pub fn demo_error_box() -> (C64, C64, C64) {
    (C64::new(0.08, -0.04), C64::new(-0.15, 0.1), C64::new(0.85, 0.2))
}

pub fn through_error_box(gamma: C64) -> C64 {
    let (e00, e11, track) = demo_error_box();
    e00 + track * gamma / (C64::new(1.0, 0.0) - e11 * gamma)
}

#[derive(Default)]
pub struct DemoVna {
    standard: Option<DemoStandard>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DemoStandard {
    Open,
    Short,
    Load,
}

impl DemoVna {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn connect_standard(&mut self, standard: Option<DemoStandard>) {
        self.standard = standard;
    }

    fn truth(&self, freq: f64) -> C64 {
        match self.standard {
            None => antenna_gamma(freq),
            Some(DemoStandard::Open) => C64::new(1.0, 0.0),
            Some(DemoStandard::Short) => C64::new(-1.0, 0.0),
            Some(DemoStandard::Load) => C64::new(0.0, 0.0),
        }
    }
}

impl Vna for DemoVna {
    fn describe(&self) -> String {
        "Demo NanoVNA (14.2 MHz dipole)".into()
    }

    fn max_points(&self) -> usize {
        101
    }

    fn range_hz(&self) -> (f64, f64) {
        (50e3, 900e6)
    }

    fn calibrated_on_device(&self) -> bool {
        false
    }

    fn scan(
        &mut self,
        start_hz: f64,
        stop_hz: f64,
        points: usize,
        _s21: bool,
    ) -> antenna_vna::Result<Vec<Point>> {
        let n = points.max(2);
        Ok((0..n)
            .map(|i| {
                let f = start_hz + (stop_hz - start_hz) * i as f64 / (n - 1) as f64;
                Point { freq: f, s11: through_error_box(self.truth(f)), s21: None }
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use antenna_vna::{Calibration, Standard};

    #[test]
    fn supply_is_cv_until_the_current_limit_then_cc() {
        let (v, a, r) = supply_solve(12.0, 5.0, true, 8.0);
        assert_eq!((v, a, r), (12.0, 1.5, Regulation::Cv));
        let (v, a, r) = supply_solve(12.0, 1.0, true, 8.0);
        assert_eq!((v, a, r), (8.0, 1.0, Regulation::Cc));
        assert_eq!(supply_solve(12.0, 1.0, false, 8.0), (0.0, 0.0, Regulation::Standby));
    }

    #[test]
    fn demo_supply_follows_set_start_and_stop() {
        let mut s = DemoSupply::new();
        s.set(10.0, 5.0).unwrap();
        assert_eq!(s.output_on().unwrap(), Some(false));
        s.start().unwrap();
        let m = s.measure().unwrap();
        assert!((m.volts - 10.0).abs() < 0.05 && (m.amps - 1.25).abs() < 0.05);
        s.stop().unwrap();
        assert_eq!(s.measure().unwrap().volts, 0.0);
    }

    #[test]
    fn demo_supply_never_exceeds_its_armed_limits() {
        let mut s = DemoSupply::new();
        assert!(s.arm(6.0, 0.5).unwrap());
        s.set(20.0, 4.0).unwrap();
        s.start().unwrap();
        let (v, a, _) = supply_solve(s.set_v, s.set_a, true, SUPPLY_OHMS);
        assert!(v <= 6.0 && a <= 0.5);
    }

    #[test]
    fn load_modes_hold_their_quantity() {
        let (v, a) = load_solve(LoadMode::Cc, 2.0, 12.0, 0.3);
        assert_eq!(a, 2.0);
        assert!((v - 11.4).abs() < 1e-9);
        let (v, a) = load_solve(LoadMode::Cv, 10.0, 12.0, 0.3);
        assert!((v - 10.0).abs() < 1e-9 && a > 0.0);
        let (v, a) = load_solve(LoadMode::Cr, 5.0, 12.0, 0.3);
        assert!((v / a - 5.0).abs() < 1e-9);
        let (v, a) = load_solve(LoadMode::Cp, 20.0, 12.0, 0.3);
        assert!((v * a - 20.0).abs() < 1e-9);
        let (_, a) = load_solve(LoadMode::Cp, 1e6, 12.0, 0.3);
        assert!((a - 20.0).abs() < 1e-9);
    }

    #[test]
    fn demo_load_counts_amp_hours_only_while_on() {
        let mut l = DemoLoad::new();
        l.set_mode(LoadMode::Cc, 3.0).unwrap();
        l.integrate(3600.0);
        assert_eq!(l.amp_hours, 0.0);
        l.on = true;
        l.integrate(1800.0);
        assert!((l.amp_hours - 1.5).abs() < 1e-9);
        assert!(l.watt_hours > 15.0);
        assert_eq!(l.runtime_s, 1800.0);
    }

    #[test]
    fn antenna_is_best_matched_at_resonance() {
        let swr = |f| Point { freq: f, s11: antenna_gamma(f), s21: None }.swr();
        assert!(swr(ANTENNA_F0) < 1.5);
        assert!(swr(10e6) > 3.0);
        assert!(swr(20e6) > 3.0);
    }

    #[test]
    fn uncalibrated_demo_sweep_is_wrong_and_sol_cal_fixes_it() {
        let mut v = DemoVna::new();
        let sweep = |v: &mut DemoVna| v.sweep(13e6, 15e6, 21, false).unwrap();
        let raw = sweep(&mut v);
        let at = 10;
        let truth = antenna_gamma(raw[at].freq);
        assert!((raw[at].s11 - truth).norm() > 0.05);

        let mut cal = Calibration::default();
        for (std, d) in [
            (Standard::Open, DemoStandard::Open),
            (Standard::Short, DemoStandard::Short),
            (Standard::Load, DemoStandard::Load),
        ] {
            v.connect_standard(Some(d));
            cal.store(std, &sweep(&mut v));
        }
        v.connect_standard(None);
        let fixed = cal.apply(&sweep(&mut v));
        assert!((fixed[at].s11 - truth).norm() < 1e-9);
    }

    #[test]
    fn demo_sweep_longer_than_one_scan_is_chunked_whole() {
        let mut v = DemoVna::new();
        let s = v.sweep(1e6, 30e6, 250, false).unwrap();
        assert_eq!(s.len(), 250);
        assert!((s[0].freq - 1e6).abs() < 1.0 && (s[249].freq - 30e6).abs() < 1.0);
    }
}
