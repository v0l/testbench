use antenna_vna::Point;

pub const Z0: f64 = 50.0;

pub fn sweep_csv(points: &[Point], z0: f64) -> String {
    let mut out = String::from("freq_hz,swr,return_loss_db,r_ohm,x_ohm,s11_re,s11_im\n");
    for p in points {
        let z = p.z(z0);
        out.push_str(&format!(
            "{:.0},{:.4},{:.3},{:.3},{:.3},{:.9},{:.9}\n",
            p.freq,
            p.swr(),
            p.return_loss_db(),
            z.re,
            z.im,
            p.s11.re,
            p.s11.im
        ));
    }
    out
}

pub fn touchstone_s1p(points: &[Point], z0: f64) -> String {
    let mut out = format!("# HZ S RI R {z0}\n");
    for p in points {
        out.push_str(&format!("{:.0} {:.9} {:.9}\n", p.freq, p.s11.re, p.s11.im));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use antenna_vna::C64;

    fn pts() -> Vec<Point> {
        vec![
            Point { freq: 1e6, s11: C64::new(0.0, 0.0), s21: None },
            Point { freq: 2e6, s11: C64::new(0.5, -0.25), s21: None },
        ]
    }

    #[test]
    fn touchstone_has_option_line_then_one_row_per_point() {
        let s = touchstone_s1p(&pts(), 50.0);
        let lines: Vec<&str> = s.lines().collect();
        assert_eq!(lines[0], "# HZ S RI R 50");
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[2], "2000000 0.500000000 -0.250000000");
    }

    #[test]
    fn csv_carries_swr_and_impedance() {
        let s = sweep_csv(&pts(), 50.0);
        let lines: Vec<&str> = s.lines().collect();
        assert_eq!(lines[0], "freq_hz,swr,return_loss_db,r_ohm,x_ohm,s11_re,s11_im");
        assert!(lines[1].starts_with("1000000,1.0000,180.000,50.000,0.000,"));
        let cols: Vec<&str> = lines[2].split(',').collect();
        assert_eq!(cols.len(), 7);
        assert!(cols[1].starts_with("3.") || cols[1].starts_with("2."));
    }
}
