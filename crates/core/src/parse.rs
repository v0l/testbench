use anyhow::{Result, bail};
use cycler_core::device::LoadMode;

pub fn parse_freq(text: &str) -> Result<f64> {
    let t = text.trim().to_ascii_lowercase();
    let t = t.strip_suffix("hz").unwrap_or(&t);
    let (num, scale) = match t.chars().last() {
        Some('k') => (&t[..t.len() - 1], 1e3),
        Some('m') => (&t[..t.len() - 1], 1e6),
        Some('g') => (&t[..t.len() - 1], 1e9),
        _ => (t, 1.0),
    };
    let Ok(v) = num.trim().parse::<f64>() else {
        bail!("not a frequency: {text:?}");
    };
    if !v.is_finite() || v < 0.0 {
        bail!("not a frequency: {text:?}");
    }
    Ok(v * scale)
}

pub fn parse_mode(text: &str) -> Result<LoadMode> {
    Ok(match text.trim().to_ascii_lowercase().as_str() {
        "cc" => LoadMode::Cc,
        "cv" => LoadMode::Cv,
        "cr" => LoadMode::Cr,
        "cp" => LoadMode::Cp,
        other => bail!("unknown load mode {other:?}; use cc, cv, cr or cp"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frequencies_take_si_suffixes() {
        assert_eq!(parse_freq("14.2M").unwrap(), 14.2e6);
        assert_eq!(parse_freq("100k").unwrap(), 100e3);
        assert_eq!(parse_freq("1.5GHz").unwrap(), 1.5e9);
        assert_eq!(parse_freq("7050000").unwrap(), 7.05e6);
        assert!(parse_freq("fast").is_err());
        assert!(parse_freq("-1M").is_err());
    }

    #[test]
    fn modes_parse_in_any_case() {
        assert_eq!(parse_mode("CP").unwrap(), LoadMode::Cp);
        assert!(parse_mode("xx").is_err());
    }
}
