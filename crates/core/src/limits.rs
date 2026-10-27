use cycler_core::device::{Limits, LoadMode};

pub const MIN_OHMS: f64 = 0.05;
pub const MAX_OHMS: f64 = 10_000.0;

fn bounded(x: f64, max: f64) -> f64 {
    let x = if x.is_finite() { x.max(0.0) } else { 0.0 };
    if max.is_finite() && max > 0.0 { x.min(max) } else { x }
}

pub fn clamp_setpoint(volts: f64, amps: f64, limits: &Limits, armed: Option<(f64, f64)>) -> (f64, f64) {
    let (mut max_v, mut max_a) = (limits.max_volts, limits.max_amps);
    if let Some((av, aa)) = armed {
        max_v = if max_v > 0.0 { max_v.min(av) } else { av };
        max_a = if max_a > 0.0 { max_a.min(aa) } else { aa };
    }
    (bounded(volts, max_v), bounded(amps, max_a))
}

pub fn clamp_load(mode: LoadMode, value: f64, limits: &Limits) -> f64 {
    match mode {
        LoadMode::Cc => bounded(value, limits.max_amps),
        LoadMode::Cv => bounded(value, limits.max_volts),
        LoadMode::Cp => bounded(value, limits.max_watts),
        LoadMode::Cr => {
            let v = if value.is_finite() { value } else { MAX_OHMS };
            v.clamp(MIN_OHMS, MAX_OHMS)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> Limits {
        Limits { max_volts: 30.0, max_amps: 5.0, max_watts: 150.0 }
    }

    #[test]
    fn setpoints_are_held_inside_the_device_limits() {
        assert_eq!(clamp_setpoint(40.0, 9.0, &limits(), None), (30.0, 5.0));
        assert_eq!(clamp_setpoint(-3.0, -1.0, &limits(), None), (0.0, 0.0));
        assert_eq!(clamp_setpoint(12.0, 1.5, &limits(), None), (12.0, 1.5));
    }

    #[test]
    fn non_finite_setpoints_become_zero() {
        assert_eq!(clamp_setpoint(f64::NAN, f64::INFINITY, &limits(), None), (0.0, 0.0));
    }

    #[test]
    fn armed_limits_are_tighter_than_the_device() {
        assert_eq!(clamp_setpoint(20.0, 4.0, &limits(), Some((5.0, 1.0))), (5.0, 1.0));
        assert_eq!(clamp_setpoint(20.0, 4.0, &limits(), Some((50.0, 9.0))), (20.0, 4.0));
    }

    #[test]
    fn unknown_device_limits_do_not_force_zero() {
        let none = Limits { max_volts: 0.0, max_amps: 0.0, max_watts: 0.0 };
        assert_eq!(clamp_setpoint(12.0, 1.0, &none, None), (12.0, 1.0));
    }

    #[test]
    fn load_setpoints_follow_their_mode() {
        let l = limits();
        assert_eq!(clamp_load(LoadMode::Cc, 9.0, &l), 5.0);
        assert_eq!(clamp_load(LoadMode::Cv, 99.0, &l), 30.0);
        assert_eq!(clamp_load(LoadMode::Cp, 999.0, &l), 150.0);
        assert_eq!(clamp_load(LoadMode::Cr, 0.0, &l), MIN_OHMS);
        assert_eq!(clamp_load(LoadMode::Cr, 1e9, &l), MAX_OHMS);
    }
}
