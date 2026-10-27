use crate::waveform::Waveform;
use std::fmt::Write;

pub fn csv(channels: &[(&str, &Waveform)]) -> String {
    let mut out = String::from("time_s");
    for (name, _) in channels {
        let _ = write!(out, ",{name}_v");
    }
    out.push('\n');
    let rows = channels.iter().map(|(_, w)| w.len()).max().unwrap_or(0);
    let Some((_, first)) = channels.first() else {
        return out;
    };
    for i in 0..rows {
        let _ = write!(out, "{:.9e}", first.time(i));
        for (_, w) in channels {
            match w.volts.get(i) {
                Some(v) => {
                    let _ = write!(out, ",{v:.6e}");
                }
                None => out.push(','),
            }
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(volts: Vec<f32>) -> Waveform {
        Waveform { t0: 0.0, dt: 1e-3, volts, vdiv: 1.0, tdiv: 1.0 }
    }

    #[test]
    fn header_and_rows() {
        let a = w(vec![1.0, 2.0]);
        let b = w(vec![3.0]);
        let s = csv(&[("ch1", &a), ("ch2", &b)]);
        let lines: Vec<&str> = s.lines().collect();
        assert_eq!(lines[0], "time_s,ch1_v,ch2_v");
        assert_eq!(lines.len(), 3);
        assert!(lines[1].starts_with("0.000000000e0,1.000000e0,3.000000e0"));
        assert!(lines[2].ends_with(','));
    }

    #[test]
    fn empty_is_header_only() {
        assert_eq!(csv(&[]), "time_s\n");
    }
}
