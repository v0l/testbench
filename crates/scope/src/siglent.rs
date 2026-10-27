use crate::block::{find_block, parse_number, text, value_after_space};
use crate::scope::{Description, Link, Scope, Slope, Trigger};
use crate::waveform::Waveform;
use anyhow::{Result, bail, ensure};

pub const GRID_DIVS: f64 = 14.0;
pub const CODES_PER_DIV: f64 = 25.0;

pub fn scale_samples(bytes: &[u8], vdiv: f64, offset: f64) -> Vec<f32> {
    bytes
        .iter()
        .map(|&b| (f64::from(b as i8) * vdiv / CODES_PER_DIV - offset) as f32)
        .collect()
}

pub fn channel_name(channel: usize) -> String {
    format!("C{}", channel + 1)
}

pub struct SiglentScope {
    link: Box<dyn Link>,
    desc: Description,
}

impl SiglentScope {
    pub fn new(link: Box<dyn Link>, desc: Description) -> Self {
        Self { link, desc }
    }

    fn check(&self, channel: usize) -> Result<()> {
        if channel >= self.desc.channels {
            bail!("channel {} out of range", channel + 1);
        }
        Ok(())
    }

    fn number(&mut self, cmd: &str) -> Result<f64> {
        let reply = self.link.query(cmd)?;
        parse_number(value_after_space(&text(&reply)))
    }
}

impl Scope for SiglentScope {
    fn describe(&self) -> &Description {
        &self.desc
    }

    fn set_timebase(&mut self, tdiv: f64) -> Result<()> {
        self.link.command(&format!("TDIV {tdiv:e}S"))
    }

    fn set_vertical(&mut self, channel: usize, vdiv: f64, offset: f64) -> Result<()> {
        self.check(channel)?;
        let ch = channel_name(channel);
        self.link.command(&format!("{ch}:VDIV {vdiv:e}V"))?;
        self.link.command(&format!("{ch}:OFST {offset:e}V"))
    }

    fn set_trigger(&mut self, t: Trigger) -> Result<()> {
        self.check(t.channel)?;
        let ch = channel_name(t.channel);
        let slope = match t.slope {
            Slope::Rising => "POS",
            Slope::Falling => "NEG",
        };
        self.link.command(&format!("TRSE EDGE,SR,{ch},HT,OFF"))?;
        self.link.command(&format!("{ch}:TRSL {slope}"))?;
        self.link.command(&format!("{ch}:TRLV {:e}V", t.level))
    }

    fn run(&mut self) -> Result<()> {
        self.link.command("TRMD AUTO")
    }

    fn stop(&mut self) -> Result<()> {
        self.link.command("STOP")
    }

    fn single(&mut self) -> Result<()> {
        self.link.command("TRMD SINGLE")
    }

    fn capture(&mut self, channel: usize) -> Result<Waveform> {
        self.check(channel)?;
        let ch = channel_name(channel);
        self.link.command("WFSU SP,1,NP,0,FP,0")?;
        let vdiv = self.number(&format!("{ch}:VDIV?"))?;
        let offset = self.number(&format!("{ch}:OFST?"))?;
        let tdiv = self.number("TDIV?")?;
        let sara = self.number("SARA?")?;
        ensure!(sara > 0.0, "scope reports a sample rate of zero");
        let reply = self.link.query(&format!("{ch}:WF? DAT2"))?;
        let bytes = find_block(&reply)?;
        ensure!(!bytes.is_empty(), "scope returned no samples");
        Ok(Waveform {
            t0: -tdiv * GRID_DIVS / 2.0,
            dt: 1.0 / sara,
            volts: scale_samples(bytes, vdiv, offset),
            vdiv,
            tdiv,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scope::parse_idn;
    use crate::scope::testing::MockLink;

    #[test]
    fn codes_are_signed_and_scaled() {
        let v = scale_samples(&[0, 25, 231, 128, 127], 0.5, 0.0);
        assert_eq!(v[0], 0.0);
        assert_eq!(v[1], 0.5);
        assert_eq!(v[2], -0.5);
        assert_eq!(v[3], -2.56);
        assert!((v[4] - 2.54).abs() < 1e-6);
    }

    #[test]
    fn offset_is_subtracted() {
        let v = scale_samples(&[25], 1.0, 0.25);
        assert_eq!(v[0], 0.75);
    }

    #[test]
    fn capture_uses_siglent_queries() {
        let mut data = b"C1:WF DAT2,#9000000003".to_vec();
        data.extend_from_slice(&[0, 25, 231]);
        data.extend_from_slice(b"\n\n");
        let mock = MockLink::default()
            .reply("C1:VDIV?", b"C1:VDIV 5.00E-01V\n")
            .reply("C1:OFST?", b"C1:OFST 0.00E+00V\n")
            .reply("TDIV?", b"TDIV 1.00E-03S\n")
            .reply("SARA?", b"SARA 1.00E+06Sa/s\n")
            .reply("C1:WF? DAT2", &data);
        let d = parse_idn("Siglent Technologies,SDS1104X-E,1,1");
        let mut s = SiglentScope::new(Box::new(mock.clone()), d);
        let w = s.capture(0).unwrap();
        assert_eq!(w.volts, vec![0.0, 0.5, -0.5]);
        assert_eq!(w.dt, 1e-6);
        assert_eq!(w.t0, -7e-3);
        assert_eq!(w.vdiv, 0.5);
        assert_eq!(w.tdiv, 1e-3);
        assert_eq!(mock.sent()[0], "WFSU SP,1,NP,0,FP,0");
    }

    #[test]
    fn controls_send_scpi() {
        let mock = MockLink::default();
        let d = parse_idn("Siglent Technologies,SDS1104X-E,1,1");
        let mut s = SiglentScope::new(Box::new(mock.clone()), d);
        s.set_vertical(1, 2.0, 0.0).unwrap();
        s.set_timebase(1e-3).unwrap();
        s.set_trigger(Trigger { channel: 0, level: 0.5, slope: Slope::Rising }).unwrap();
        s.run().unwrap();
        s.stop().unwrap();
        s.single().unwrap();
        let sent = mock.sent();
        assert!(sent.contains(&"C2:VDIV 2e0V".to_string()));
        assert!(sent.contains(&"TDIV 1e-3S".to_string()));
        assert!(sent.contains(&"C1:TRSL POS".to_string()));
        assert!(sent.contains(&"TRMD SINGLE".to_string()));
        assert!(s.capture(7).is_err());
    }
}
