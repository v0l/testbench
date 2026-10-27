use anyhow::{Result, bail, ensure};

pub fn parse_block(data: &[u8]) -> Result<&[u8]> {
    ensure!(data.first() == Some(&b'#'), "missing # in block header");
    let digits = match data.get(1) {
        Some(d @ b'1'..=b'9') => (d - b'0') as usize,
        Some(b'0') => bail!("indefinite length block is not supported"),
        _ => bail!("bad block header digit count"),
    };
    ensure!(data.len() >= 2 + digits, "block header truncated");
    let len: usize = std::str::from_utf8(&data[2..2 + digits])?.parse()?;
    let start = 2 + digits;
    ensure!(
        data.len() >= start + len,
        "block holds {} bytes but header says {}",
        data.len() - start,
        len
    );
    Ok(&data[start..start + len])
}

pub fn find_block(data: &[u8]) -> Result<&[u8]> {
    let at = data
        .iter()
        .position(|&b| b == b'#')
        .ok_or_else(|| anyhow::anyhow!("no block in reply"))?;
    parse_block(&data[at..])
}

pub fn text(data: &[u8]) -> String {
    String::from_utf8_lossy(data).trim().to_string()
}

pub fn parse_number(s: &str) -> Result<f64> {
    let s = s.trim();
    let end = s
        .char_indices()
        .take_while(|(i, c)| {
            c.is_ascii_digit()
                || matches!(c, '.' | '+' | '-')
                || (matches!(c, 'e' | 'E')
                    && s[i + 1..]
                        .chars()
                        .next()
                        .is_some_and(|n| n.is_ascii_digit() || n == '+' || n == '-'))
        })
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    ensure!(end > 0, "no number in {s:?}");
    Ok(s[..end].parse()?)
}

pub fn value_after_space(reply: &str) -> &str {
    match reply.split_once(' ') {
        Some((_, v)) => v.trim(),
        None => reply.trim(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_with_nine_digits() {
        let mut d = b"#9000000005".to_vec();
        d.extend_from_slice(&[1, 2, 3, 4, 5]);
        d.push(b'\n');
        assert_eq!(parse_block(&d).unwrap(), &[1, 2, 3, 4, 5]);
    }

    #[test]
    fn block_with_short_header() {
        assert_eq!(parse_block(b"#14abcdEXTRA").unwrap(), b"abcd");
        assert_eq!(parse_block(b"#210abcdefghij").unwrap(), b"abcdefghij");
    }

    #[test]
    fn block_errors() {
        assert!(parse_block(b"x#14abcd").is_err());
        assert!(parse_block(b"#0abcd").is_err());
        assert!(parse_block(b"#14ab").is_err());
        assert!(parse_block(b"#9000").is_err());
        assert!(parse_block(b"#1x").is_err());
        assert!(parse_block(b"").is_err());
    }

    #[test]
    fn block_may_contain_hash_and_newlines() {
        let d = b"#14#\n#\n";
        assert_eq!(parse_block(d).unwrap(), b"#\n#\n");
    }

    #[test]
    fn find_block_skips_prefix() {
        assert_eq!(find_block(b"C1:WF DAT2,#14abcd\n\n").unwrap(), b"abcd");
        assert!(find_block(b"no block").is_err());
    }

    #[test]
    fn numbers_with_units() {
        assert_eq!(parse_number("5.00E-01V").unwrap(), 0.5);
        assert_eq!(parse_number("1.00E+09Sa/s").unwrap(), 1e9);
        assert_eq!(parse_number(" -2.5e-3 ").unwrap(), -2.5e-3);
        assert_eq!(parse_number("12").unwrap(), 12.0);
        assert_eq!(parse_number("5E-06S").unwrap(), 5e-6);
        assert!(parse_number("V").is_err());
    }

    #[test]
    fn value_after_the_command_echo() {
        assert_eq!(value_after_space("C1:VDIV 5.00E-01V"), "5.00E-01V");
        assert_eq!(value_after_space("TDIV 1E-3S\n"), "1E-3S");
        assert_eq!(value_after_space("0.5"), "0.5");
    }
}
