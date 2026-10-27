use antenna_vna::C64;

pub fn clip(g: C64) -> C64 {
    let n = g.norm();
    if n > 1.0 { g / n } else { g }
}

pub fn to_screen(g: C64, centre: (f64, f64), radius: f64) -> (f64, f64) {
    let g = clip(g);
    (centre.0 + g.re * radius, centre.1 - g.im * radius)
}

pub fn gamma_of(z_norm: C64) -> C64 {
    let one = C64::new(1.0, 0.0);
    (z_norm - one) / (z_norm + one)
}

pub fn resistance_circle(r: f64) -> (C64, f64) {
    (C64::new(r / (1.0 + r), 0.0), 1.0 / (1.0 + r))
}

pub fn reactance_circle(x: f64) -> (C64, f64) {
    (C64::new(1.0, 1.0 / x), 1.0 / x.abs())
}

pub fn reactance_arc(x: f64, steps: usize) -> Vec<C64> {
    (0..=steps)
        .map(|i| {
            let t = i as f64 / steps as f64;
            let r = (t / (1.0 - t).max(1e-6)).powi(2) * 0.05;
            gamma_of(C64::new(r, x))
        })
        .collect()
}

pub const GRID_R: [f64; 5] = [0.2, 0.5, 1.0, 2.0, 5.0];
pub const GRID_X: [f64; 5] = [0.2, 0.5, 1.0, 2.0, 5.0];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matched_load_sits_at_the_centre() {
        assert_eq!(to_screen(C64::new(0.0, 0.0), (100.0, 100.0), 50.0), (100.0, 100.0));
    }

    #[test]
    fn open_is_right_short_is_left_and_positive_reactance_is_up() {
        assert_eq!(to_screen(C64::new(1.0, 0.0), (0.0, 0.0), 10.0), (10.0, 0.0));
        assert_eq!(to_screen(C64::new(-1.0, 0.0), (0.0, 0.0), 10.0), (-10.0, 0.0));
        let (_, y) = to_screen(C64::new(0.0, 0.5), (0.0, 0.0), 10.0);
        assert_eq!(y, -5.0);
    }

    #[test]
    fn gamma_outside_the_unit_circle_is_pulled_onto_it() {
        let (x, y) = to_screen(C64::new(3.0, 4.0), (0.0, 0.0), 10.0);
        assert!(((x * x + y * y).sqrt() - 10.0).abs() < 1e-9);
    }

    #[test]
    fn unit_resistance_circle_passes_through_the_centre() {
        let (c, r) = resistance_circle(1.0);
        assert!((c - C64::new(0.5, 0.0)).norm() - r < 1e-12);
        assert!(((C64::new(0.0, 0.0) - c).norm() - r).abs() < 1e-12);
        assert!(((C64::new(1.0, 0.0) - c).norm() - r).abs() < 1e-12);
    }

    #[test]
    fn reactance_arc_points_lie_on_the_reactance_circle() {
        for x in [0.5, 1.0, -2.0] {
            let (c, r) = reactance_circle(x);
            for g in reactance_arc(x, 40) {
                assert!(((g - c).norm() - r).abs() < 1e-6, "x={x} g={g}");
                assert!(g.norm() <= 1.0 + 1e-9);
            }
        }
    }

    #[test]
    fn resistance_circle_matches_the_impedance_map() {
        let (c, r) = resistance_circle(2.0);
        let g = gamma_of(C64::new(2.0, 1.3));
        assert!(((g - c).norm() - r).abs() < 1e-12);
    }
}
