use crate::block::{find_block, parse_number, text};
use crate::scope::{Description, Link, Scope, Slope, Trigger};
use crate::waveform::Waveform;
use anyhow::{Result, bail, ensure};

pub const CHUNK: usize = 250_000;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Preamble {
    pub format: u32,
    pub kind: u32,
    pub points: usize,
    pub count: u32,
    pub xincrement: f64,
    pub xorigin: f64,
    pub xreference: f64,
    pub yincrement: f64,
    pub yorigin: f64,
    pub yreference: f64,
}

pub fn parse_preamble(s: &str) -> Result<Preamble> {
    let f: Vec<&str> = s.trim().split(',').map(str::trim).collect();
    ensure!(f.len() >= 10, "preamble has {} fields, expected 10", f.len());
    let n = |i: usize| -> Result<f64> { parse_number(f[i]) };
    let p = Preamble {
        format: n(0)? as u32,
        kind: n(1)? as u32,
        points: n(2)? as usize,
        count: n(3)? as u32,
        xincrement: n(4)?,
        xorigin: n(5)?,
        xreference: n(6)?,
        yincrement: n(7)?,
        yorigin: n(8)?,
        yreference: n(9)?,
    };
    ensure!(p.xincrement > 0.0, "preamble xincrement is not positive");
    Ok(p)
}

pub fn scale_samples(bytes: &[u8], p: &Preamble) -> Vec<f32> {
    bytes
        .iter()
        .map(|&b| ((f64::from(b) - p.yorigin - p.yreference) * p.yincrement) as f32)
        .collect()
}

pub fn chunk_ranges(points: usize, chunk: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = 1;
    while start <= points {
        let stop = (start + chunk - 1).min(points);
        out.push((start, stop));
        start = stop + 1;
    }
    out
}

pub fn channel_name(channel: usize) -> String {
    format!("CHAN{}", channel + 1)
}

pub struct WavScope {
    link: Box<dyn Link>,
    desc: Description,
    chunked: bool,
    pub raw: bool,
}

impl WavScope {
    pub fn new(link: Box<dyn Link>, desc: Description, chunked: bool) -> Self {
        Self { link, desc, chunked, raw: false }
    }

    fn check(&self, channel: usize) -> Result<()> {
        if channel >= self.desc.channels {
            bail!("channel {} out of range", channel + 1);
        }
        Ok(())
    }

    fn number(&mut self, cmd: &str) -> Result<f64> {
        let reply = self.link.query(cmd)?;
        parse_number(&text(&reply))
    }
}

impl Scope for WavScope {
    fn describe(&self) -> &Description {
        &self.desc
    }

    fn set_timebase(&mut self, tdiv: f64) -> Result<()> {
        self.link.command(&format!(":TIM:SCAL {tdiv:e}"))
    }

    fn set_vertical(&mut self, channel: usize, vdiv: f64, offset: f64) -> Result<()> {
        self.check(channel)?;
        let ch = channel_name(channel);
        self.link.command(&format!(":{ch}:SCAL {vdiv:e}"))?;
        self.link.command(&format!(":{ch}:OFFS {offset:e}"))
    }

    fn set_trigger(&mut self, t: Trigger) -> Result<()> {
        self.check(t.channel)?;
        let slope = match t.slope {
            Slope::Rising => "POS",
            Slope::Falling => "NEG",
        };
        self.link.command(&format!(":TRIG:EDGE:SOUR {}", channel_name(t.channel)))?;
        self.link.command(&format!(":TRIG:EDGE:SLOP {slope}"))?;
        self.link.command(&format!(":TRIG:EDGE:LEV {:e}", t.level))
    }

    fn run(&mut self) -> Result<()> {
        self.link.command(":RUN")
    }

    fn stop(&mut self) -> Result<()> {
        self.link.command(":STOP")
    }

    fn single(&mut self) -> Result<()> {
        self.link.command(":SING")
    }

    fn capture(&mut self, channel: usize) -> Result<Waveform> {
        self.check(channel)?;
        let ch = channel_name(channel);
        self.link.command(&format!(":WAV:SOUR {ch}"))?;
        self.link.command(if self.raw { ":WAV:MODE RAW" } else { ":WAV:MODE NORM" })?;
        self.link.command(":WAV:FORM BYTE")?;
        let pre = parse_preamble(&text(&self.link.query(":WAV:PRE?")?))?;
        let mut bytes = Vec::with_capacity(pre.points);
        if self.chunked {
            for (start, stop) in chunk_ranges(pre.points, CHUNK) {
                self.link.command(&format!(":WAV:STAR {start}"))?;
                self.link.command(&format!(":WAV:STOP {stop}"))?;
                let reply = self.link.query(":WAV:DATA?")?;
                bytes.extend_from_slice(find_block(&reply)?);
            }
        } else {
            let reply = self.link.query(":WAV:DATA?")?;
            bytes.extend_from_slice(find_block(&reply)?);
        }
        ensure!(!bytes.is_empty(), "scope returned no samples");
        let vdiv = self.number(&format!(":{ch}:SCAL?"))?;
        let tdiv = self.number(":TIM:SCAL?")?;
        Ok(Waveform {
            t0: pre.xorigin - pre.xreference * pre.xincrement,
            dt: pre.xincrement,
            volts: scale_samples(&bytes, &pre),
            vdiv,
            tdiv,
        })
    }

    fn screenshot(&mut self) -> Result<Option<Vec<u8>>> {
        let reply = self.link.query(":DISP:DATA? ON,OFF,PNG")?;
        Ok(Some(find_block(&reply)?.to_vec()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scope::testing::MockLink;
    use crate::scope::parse_idn;

    const PRE: &str = "0,2,4,1,2.000000e-06,-1.000000e-05,0,4.000000e-02,127,0\n";

    fn block(data: &[u8]) -> Vec<u8> {
        let mut v = format!("#9{:09}", data.len()).into_bytes();
        v.extend_from_slice(data);
        v.push(b'\n');
        v
    }

    #[test]
    fn preamble_parses() {
        let p = parse_preamble(PRE).unwrap();
        assert_eq!(p.points, 4);
        assert_eq!(p.xincrement, 2e-6);
        assert_eq!(p.xorigin, -1e-5);
        assert_eq!(p.yincrement, 0.04);
        assert_eq!(p.yorigin, 127.0);
    }

    #[test]
    fn preamble_rejects_short_or_bad() {
        assert!(parse_preamble("0,2,4").is_err());
        assert!(parse_preamble("0,2,4,1,0,0,0,1,0,0").is_err());
        assert!(parse_preamble("a,b,c,d,e,f,g,h,i,j").is_err());
    }

    #[test]
    fn samples_scale_with_origin_and_reference() {
        let p = parse_preamble(PRE).unwrap();
        let v = scale_samples(&[127, 137, 117, 255], &p);
        assert!((v[0] - 0.0).abs() < 1e-6);
        assert!((v[1] - 0.4).abs() < 1e-6);
        assert!((v[2] + 0.4).abs() < 1e-6);
        assert!((v[3] - 5.12).abs() < 1e-5);
    }

    #[test]
    fn chunking_covers_every_point_once() {
        assert_eq!(chunk_ranges(10, 4), vec![(1, 4), (5, 8), (9, 10)]);
        assert_eq!(chunk_ranges(4, 4), vec![(1, 4)]);
        assert!(chunk_ranges(0, 4).is_empty());
        let r = chunk_ranges(1_200_000, CHUNK);
        assert_eq!(r.len(), 5);
        assert_eq!(r.last().unwrap().1, 1_200_000);
    }

    fn scope(mock: MockLink, chunked: bool) -> WavScope {
        let d = parse_idn("RIGOL TECHNOLOGIES,DS1054Z,1,1");
        WavScope::new(Box::new(mock), d, chunked)
    }

    #[test]
    fn capture_runs_the_rigol_sequence() {
        let mock = MockLink::default()
            .reply(":WAV:PRE?", PRE.as_bytes())
            .reply(":WAV:DATA?", &block(&[127, 137, 117, 255]))
            .reply(":CHAN2:SCAL?", b"5.000000e-01\n")
            .reply(":TIM:SCAL?", b"1.000000e-05\n");
        let mut s = scope(mock.clone(), true);
        let w = s.capture(1).unwrap();
        assert_eq!(w.len(), 4);
        assert_eq!(w.dt, 2e-6);
        assert_eq!(w.t0, -1e-5);
        assert_eq!(w.vdiv, 0.5);
        assert_eq!(w.tdiv, 1e-5);
        let sent = mock.sent();
        assert_eq!(sent[0], ":WAV:SOUR CHAN2");
        assert!(sent.contains(&":WAV:MODE NORM".to_string()));
        assert!(sent.contains(&":WAV:FORM BYTE".to_string()));
        assert!(sent.contains(&":WAV:STAR 1".to_string()));
        assert!(sent.contains(&":WAV:STOP 4".to_string()));
    }

    #[test]
    fn raw_mode_and_non_chunked_path() {
        let mock = MockLink::default()
            .reply(":WAV:PRE?", PRE.as_bytes())
            .reply(":WAV:DATA?", &block(&[1, 2, 3, 4]))
            .reply(":CHAN1:SCAL?", b"1\n")
            .reply(":TIM:SCAL?", b"1\n");
        let mut s = scope(mock.clone(), false);
        s.raw = true;
        s.capture(0).unwrap();
        let sent = mock.sent();
        assert!(sent.contains(&":WAV:MODE RAW".to_string()));
        assert!(!sent.iter().any(|c| c.starts_with(":WAV:STAR")));
    }

    #[test]
    fn two_chunks_are_joined() {
        let pre = format!("0,2,{},1,1e-6,0,0,1,0,0\n", CHUNK + 2);
        let first = vec![1u8; CHUNK];
        let mock = MockLink::default()
            .reply(":WAV:PRE?", pre.as_bytes())
            .reply(":WAV:DATA?", &block(&first))
            .reply(":WAV:DATA?", &block(&[2, 2]))
            .reply(":CHAN1:SCAL?", b"1\n")
            .reply(":TIM:SCAL?", b"1\n");
        let mut s = scope(mock.clone(), true);
        let w = s.capture(0).unwrap();
        assert_eq!(w.len(), CHUNK + 2);
        assert_eq!(w.volts[CHUNK], 2.0);
        assert!(mock.sent().contains(&format!(":WAV:STAR {}", CHUNK + 1)));
    }

    #[test]
    fn controls_send_scpi() {
        let mock = MockLink::default();
        let mut s = scope(mock.clone(), true);
        s.set_vertical(0, 0.5, 0.0).unwrap();
        s.set_timebase(1e-3).unwrap();
        s.set_trigger(Trigger { channel: 1, level: 0.25, slope: Slope::Falling }).unwrap();
        s.run().unwrap();
        s.stop().unwrap();
        s.single().unwrap();
        let sent = mock.sent();
        assert!(sent.contains(&":CHAN1:SCAL 5e-1".to_string()));
        assert!(sent.contains(&":TIM:SCAL 1e-3".to_string()));
        assert!(sent.contains(&":TRIG:EDGE:SOUR CHAN2".to_string()));
        assert!(sent.contains(&":TRIG:EDGE:SLOP NEG".to_string()));
        assert!(sent.contains(&":SING".to_string()));
        assert!(s.set_vertical(9, 1.0, 0.0).is_err());
    }
}
