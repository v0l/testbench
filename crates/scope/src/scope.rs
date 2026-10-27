use crate::siglent::SiglentScope;
use crate::usbtmc::{Pipe, UsbTmc};
use crate::wav::WavScope;
use crate::waveform::Waveform;
use anyhow::{Result, bail};

pub trait Link: Send {
    fn command(&mut self, cmd: &str) -> Result<()>;
    fn query(&mut self, cmd: &str) -> Result<Vec<u8>>;
}

impl<P: Pipe> Link for UsbTmc<P> {
    fn command(&mut self, cmd: &str) -> Result<()> {
        self.send(cmd.as_bytes())
    }

    fn query(&mut self, cmd: &str) -> Result<Vec<u8>> {
        self.send(cmd.as_bytes())?;
        self.receive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slope {
    Rising,
    Falling,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Trigger {
    pub channel: usize,
    pub level: f64,
    pub slope: Slope,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Description {
    pub manufacturer: String,
    pub model: String,
    pub serial: String,
    pub firmware: String,
    pub channels: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    Rigol,
    Siglent,
    Wav,
}

pub trait Scope: Send {
    fn describe(&self) -> &Description;
    fn channels(&self) -> usize {
        self.describe().channels
    }
    fn set_timebase(&mut self, tdiv: f64) -> Result<()>;
    fn set_vertical(&mut self, channel: usize, vdiv: f64, offset: f64) -> Result<()>;
    fn set_trigger(&mut self, trigger: Trigger) -> Result<()>;
    fn run(&mut self) -> Result<()>;
    fn stop(&mut self) -> Result<()>;
    fn single(&mut self) -> Result<()>;
    fn capture(&mut self, channel: usize) -> Result<Waveform>;
    fn screenshot(&mut self) -> Result<Option<Vec<u8>>> {
        Ok(None)
    }
}

pub fn parse_idn(reply: &str) -> Description {
    let mut parts = reply.trim().split(',').map(|s| s.trim().to_string());
    let manufacturer = parts.next().unwrap_or_default();
    let model = parts.next().unwrap_or_default();
    let serial = parts.next().unwrap_or_default();
    let firmware = parts.collect::<Vec<_>>().join(",");
    let channels = channels_from_model(&model);
    Description { manufacturer, model, serial, firmware, channels }
}

pub fn channels_from_model(model: &str) -> usize {
    let digits: String = model
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    match digits.chars().last().and_then(|c| c.to_digit(10)) {
        Some(n @ 1..=4) => n as usize,
        _ => 4,
    }
}

pub fn dialect_for(manufacturer: &str) -> Option<Dialect> {
    let m = manufacturer.to_ascii_uppercase();
    if m.contains("RIGOL") {
        Some(Dialect::Rigol)
    } else if m.contains("SIGLENT") {
        Some(Dialect::Siglent)
    } else if ["KEYSIGHT", "AGILENT", "OWON", "HANTEK"].iter().any(|v| m.contains(v)) {
        Some(Dialect::Wav)
    } else {
        None
    }
}

pub fn connect(mut link: Box<dyn Link>) -> Result<Box<dyn Scope>> {
    let reply = link.query("*IDN?")?;
    let desc = parse_idn(&crate::block::text(&reply));
    match dialect_for(&desc.manufacturer) {
        Some(Dialect::Rigol) => Ok(Box::new(WavScope::new(link, desc, true))),
        Some(Dialect::Wav) => Ok(Box::new(WavScope::new(link, desc, false))),
        Some(Dialect::Siglent) => Ok(Box::new(SiglentScope::new(link, desc))),
        None => bail!("no driver for {} {}", desc.manufacturer, desc.model),
    }
}

pub fn connect_usb(info: &crate::usb::UsbtmcInfo) -> Result<Box<dyn Scope>> {
    if crate::usb::is_hantek(info.vendor_id, info.product_id) {
        let pipe = crate::usb::open_pipe(info, 4096, crate::hantek::READ_TIMEOUT)?;
        return Ok(Box::new(crate::hantek::HantekScope::new(pipe)?));
    }
    connect(Box::new(crate::usb::open_usb(info)?))
}

#[cfg(test)]
pub mod testing {
    use super::*;
    use std::sync::{Arc, Mutex};

    pub type Scripted = (String, Vec<u8>);

    #[derive(Clone, Default)]
    pub struct MockLink {
        pub sent: Arc<Mutex<Vec<String>>>,
        pub replies: Arc<Mutex<Vec<Scripted>>>,
    }

    impl MockLink {
        pub fn reply(self, cmd: &str, data: &[u8]) -> Self {
            self.replies.lock().unwrap().push((cmd.to_string(), data.to_vec()));
            self
        }

        pub fn sent(&self) -> Vec<String> {
            self.sent.lock().unwrap().clone()
        }
    }

    impl Link for MockLink {
        fn command(&mut self, cmd: &str) -> Result<()> {
            self.sent.lock().unwrap().push(cmd.to_string());
            Ok(())
        }

        fn query(&mut self, cmd: &str) -> Result<Vec<u8>> {
            self.sent.lock().unwrap().push(cmd.to_string());
            let mut r = self.replies.lock().unwrap();
            let at = r.iter().position(|(c, _)| c == cmd);
            match at {
                Some(i) if r.iter().filter(|(c, _)| c == cmd).count() > 1 => Ok(r.remove(i).1),
                Some(i) => Ok(r[i].1.clone()),
                None => bail!("unscripted query {cmd}"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::MockLink;
    use super::*;

    #[test]
    fn idn_fields() {
        let d = parse_idn("RIGOL TECHNOLOGIES,DS1054Z,DS1ZA1234,00.04.04.SP3\n");
        assert_eq!(d.manufacturer, "RIGOL TECHNOLOGIES");
        assert_eq!(d.model, "DS1054Z");
        assert_eq!(d.serial, "DS1ZA1234");
        assert_eq!(d.firmware, "00.04.04.SP3");
        assert_eq!(d.channels, 4);
    }

    #[test]
    fn channel_counts() {
        assert_eq!(channels_from_model("DS1102Z-E"), 2);
        assert_eq!(channels_from_model("DHO804"), 4);
        assert_eq!(channels_from_model("SDS1202X-E"), 2);
        assert_eq!(channels_from_model("SDS1104X-E"), 4);
        assert_eq!(channels_from_model("MSO"), 4);
    }

    #[test]
    fn dialect_selection() {
        assert_eq!(dialect_for("Rigol Technologies"), Some(Dialect::Rigol));
        assert_eq!(dialect_for("Siglent Technologies"), Some(Dialect::Siglent));
        assert_eq!(dialect_for("KEYSIGHT TECHNOLOGIES"), Some(Dialect::Wav));
        assert_eq!(dialect_for("OWON"), Some(Dialect::Wav));
        assert_eq!(dialect_for("Acme"), None);
    }

    #[test]
    fn connect_picks_driver_by_idn() {
        let m = MockLink::default().reply("*IDN?", b"Siglent Technologies,SDS1104X-E,SDS1,1.0\n");
        let s = connect(Box::new(m)).unwrap();
        assert_eq!(s.describe().model, "SDS1104X-E");
        assert_eq!(s.channels(), 4);
        let m = MockLink::default().reply("*IDN?", b"Nobody,X1,1,1\n");
        assert!(connect(Box::new(m)).is_err());
    }
}
