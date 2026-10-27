use crate::scope::{Description, Scope, Trigger};
use crate::usbtmc::Pipe;
use crate::waveform::Waveform;
use anyhow::{Result, bail, ensure};
use std::time::Duration;

pub const COUNTS_PER_DIV: f64 = 25.0;
pub const SAMPLES_PER_DIV: f64 = 200.0;
pub const SETTINGS_LEN: usize = 208;
pub const SCREEN_W: usize = 800;
pub const SCREEN_H: usize = 480;

const CMD_ECHO: u8 = 0x00;
const CMD_SETTINGS: u8 = 0x01;
const CMD_SAMPLES: u8 = 0x02;
const CMD_ACQUISITION: u8 = 0x12;
const CMD_SCREENSHOT: u8 = 0x20;
const REPLY: u8 = 0x80;
pub const READ_TIMEOUT: Duration = Duration::from_millis(1500);
const RETRIES: usize = 6;
const SETTLE: Duration = Duration::from_millis(100);
const RETRY_WAIT: Duration = Duration::from_millis(25);

pub const VDIV: [f64; 11] = [0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0];

pub const TDIV: [f64; 32] = [
    2e-9, 4e-9, 8e-9, 2e-8, 4e-8, 8e-8, 2e-7, 4e-7, 8e-7, 2e-6, 4e-6, 8e-6, 2e-5, 4e-5, 8e-5,
    2e-4, 4e-4, 8e-4, 2e-3, 4e-3, 8e-3, 2e-2, 4e-2, 8e-2, 0.2, 0.4, 0.8, 2.0, 4.0, 8.0, 20.0,
    40.0,
];

pub fn frame(cmd: u8, payload: &[u8]) -> Vec<u8> {
    let body = payload.len() + 2;
    let mut out = vec![0x53, (body & 0xff) as u8, (body >> 8) as u8, cmd];
    out.extend_from_slice(payload);
    out.push(checksum(&out));
    out
}

pub fn checksum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0u8, |a, b| a.wrapping_add(*b))
}

#[derive(Debug, Clone, PartialEq)]
pub struct Reply<'a> {
    pub cmd: u8,
    pub body: &'a [u8],
}

pub fn parse_frame(buf: &[u8]) -> Result<Reply<'_>> {
    ensure!(buf.len() >= 5, "Hantek reply of {} bytes is too short", buf.len());
    ensure!(buf[0] == 0x53, "Hantek reply starts with {:#04x}, not 0x53", buf[0]);
    let body = usize::from(buf[1]) | usize::from(buf[2]) << 8;
    ensure!(buf.len() == body + 3, "Hantek reply is {} bytes, header says {}", buf.len(), body + 3);
    let (data, sum) = buf.split_at(buf.len() - 1);
    ensure!(checksum(data) == sum[0], "Hantek reply checksum mismatch");
    Ok(Reply { cmd: buf[3], body: &buf[4..buf.len() - 1] })
}

pub fn probe_from_index(i: u8) -> f64 {
    match i {
        0 | 1 => 1.0,
        2 => 10.0,
        3 => 100.0,
        _ => 1000.0,
    }
}

pub fn declared_len(body: &[u8]) -> usize {
    match body {
        [_, a, b, c, ..] => usize::from(*a) | usize::from(*b) << 8 | usize::from(*c) << 16,
        _ => usize::MAX,
    }
}

pub fn vdiv_from_index(i: u8) -> Option<f64> {
    VDIV.get(usize::from(i)).copied()
}

pub fn tdiv_from_index(i: u8) -> Option<f64> {
    TDIV.get(usize::from(i)).copied()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Channel {
    pub on: bool,
    pub vdiv: f64,
    pub probe: f64,
    pub position: i16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub channels: [Channel; 2],
    pub tdiv: f64,
}

pub fn parse_settings(raw: &[u8]) -> Result<Settings> {
    ensure!(raw.len() >= SETTINGS_LEN, "settings blob is {} bytes, expected {SETTINGS_LEN}", raw.len());
    let channel = |base: usize| -> Result<Channel> {
        let Some(vdiv) = vdiv_from_index(raw[base + 1]) else {
            bail!("unknown volts per division index {}", raw[base + 1]);
        };
        let probe = probe_from_index(raw[base + 5]);
        Ok(Channel {
            on: raw[base] != 0,
            vdiv,
            probe,
            position: i16::from_le_bytes([raw[base + 8], raw[base + 9]]),
        })
    };
    let Some(tdiv) = tdiv_from_index(raw[160]) else {
        bail!("unknown time per division index {}", raw[160]);
    };
    Ok(Settings { channels: [channel(0)?, channel(10)?], tdiv })
}

pub fn scale_samples(raw: &[u8], vdiv: f64, probe: f64, position: i16) -> Vec<f32> {
    raw.iter()
        .map(|&b| {
            let counts = f64::from(b as i8) - f64::from(position);
            (counts / COUNTS_PER_DIV * vdiv * probe) as f32
        })
        .collect()
}

pub fn rgb565_to_rgb(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len() / 2 * 3);
    for p in raw.as_chunks::<2>().0 {
        let v = u16::from(p[0]) | u16::from(p[1]) << 8;
        let r = u32::from(v >> 11) & 31;
        let g = u32::from(v >> 5) & 63;
        let b = u32::from(v) & 31;
        out.push((r * 255 / 31) as u8);
        out.push((g * 255 / 63) as u8);
        out.push((b * 255 / 31) as u8);
    }
    out
}

pub fn png_from_rgb(rgb: &[u8], width: usize, height: usize) -> Result<Vec<u8>> {
    ensure!(rgb.len() == width * height * 3, "screen data is {} bytes", rgb.len());
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, width as u32, height as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(rgb)?;
    Ok(out)
}

pub struct HantekScope<P: Pipe> {
    pipe: P,
    desc: Description,
    running: bool,
}

impl<P: Pipe> HantekScope<P> {
    pub fn new(pipe: P) -> Result<Self> {
        let mut s = Self {
            pipe,
            desc: Description {
                manufacturer: "Hantek".into(),
                model: "DSO5102P".into(),
                serial: String::new(),
                firmware: String::new(),
                channels: 2,
            },
            running: true,
        };
        s.exchange(CMD_ECHO, b"tb")?;
        s.settings()?;
        Ok(s)
    }

    fn send(&mut self, cmd: u8, payload: &[u8]) -> Result<()> {
        std::thread::sleep(SETTLE);
        self.pipe.write(&frame(cmd, payload))
    }

    fn next(&mut self, cmd: u8) -> Result<Vec<u8>> {
        loop {
            let buf = self.pipe.read()?;
            let reply = parse_frame(&buf)?;
            if reply.cmd == cmd | REPLY {
                return Ok(reply.body.to_vec());
            }
        }
    }

    fn exchange(&mut self, cmd: u8, payload: &[u8]) -> Result<Vec<u8>> {
        let mut last = None;
        for _ in 0..RETRIES {
            self.send(cmd, payload)?;
            match self.next(cmd) {
                Ok(body) => return Ok(body),
                Err(e) => last = Some(e),
            }
        }
        Err(last.unwrap_or_else(|| anyhow::anyhow!("no reply")))
    }

    fn settings(&mut self) -> Result<Settings> {
        let raw = self.exchange(CMD_SETTINGS, &[])?;
        parse_settings(&raw)
    }

    fn read_samples(&mut self, channel: usize) -> Result<Vec<u8>> {
        for _ in 0..RETRIES {
            self.send(CMD_SAMPLES, &[1, channel as u8])?;
            let mut data = Vec::new();
            let mut got = false;
            let mut expected = usize::MAX;
            loop {
                let Ok(body) = self.next(CMD_SAMPLES) else { break };
                match body.first() {
                    Some(0) => expected = declared_len(&body),
                    Some(1) => {
                        got = true;
                        data.extend_from_slice(body.get(2..).unwrap_or_default());
                    }
                    _ => break,
                }
            }
            if got && data.len() == expected {
                return Ok(data);
            }
            std::thread::sleep(RETRY_WAIT);
        }
        bail!("scope returned no samples for channel {}", channel + 1)
    }
}

impl<P: Pipe> Scope for HantekScope<P> {
    fn describe(&self) -> &Description {
        &self.desc
    }

    fn set_timebase(&mut self, _tdiv: f64) -> Result<()> {
        Ok(())
    }

    fn set_vertical(&mut self, _channel: usize, _vdiv: f64, _offset: f64) -> Result<()> {
        Ok(())
    }

    fn set_trigger(&mut self, _trigger: Trigger) -> Result<()> {
        Ok(())
    }

    fn run(&mut self) -> Result<()> {
        self.exchange(CMD_ACQUISITION, &[0, 0])?;
        self.running = true;
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        self.exchange(CMD_ACQUISITION, &[0, 1])?;
        self.running = false;
        Ok(())
    }

    fn single(&mut self) -> Result<()> {
        self.stop()
    }

    fn capture(&mut self, channel: usize) -> Result<Waveform> {
        ensure!(channel < 2, "the DSO5102P has two channels");
        let settings = self.settings()?;
        let ch = settings.channels[channel];
        ensure!(ch.on, "channel {} is switched off on the scope", channel + 1);
        let raw = self.read_samples(channel)?;
        let n = raw.len() as f64;
        let dt = settings.tdiv / SAMPLES_PER_DIV;
        Ok(Waveform {
            t0: -dt * n / 2.0,
            dt,
            volts: scale_samples(&raw, ch.vdiv, ch.probe, ch.position),
            vdiv: ch.vdiv * ch.probe,
            tdiv: settings.tdiv,
        })
    }

    fn screenshot(&mut self) -> Result<Option<Vec<u8>>> {
        self.send(CMD_SCREENSHOT, &[])?;
        let mut data = Vec::new();
        loop {
            let body = self.next(CMD_SCREENSHOT)?;
            match body.first() {
                Some(0) => {}
                Some(1) => data.extend_from_slice(&body[1..]),
                _ => break,
            }
        }
        ensure!(data.len() == SCREEN_W * SCREEN_H * 2, "screenshot is {} bytes", data.len());
        Ok(Some(png_from_rgb(&rgb565_to_rgb(&data), SCREEN_W, SCREEN_H)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    struct Fake {
        replies: VecDeque<Vec<u8>>,
        sent: Vec<Vec<u8>>,
    }

    impl Pipe for Fake {
        fn write(&mut self, data: &[u8]) -> Result<()> {
            self.sent.push(data.to_vec());
            Ok(())
        }
        fn read(&mut self) -> Result<Vec<u8>> {
            self.replies.pop_front().ok_or_else(|| anyhow::anyhow!("no more replies"))
        }
    }

    struct Flaky {
        inner: Fake,
        drop_first: bool,
    }

    impl Pipe for Flaky {
        fn write(&mut self, data: &[u8]) -> Result<()> {
            self.inner.write(data)
        }
        fn read(&mut self) -> Result<Vec<u8>> {
            if self.drop_first {
                self.drop_first = false;
                anyhow::bail!("timed out");
            }
            self.inner.read()
        }
    }

    fn settings_blob(vb: u8, pos: i16, tb: u8, on: u8) -> Vec<u8> {
        let mut b = vec![0u8; SETTINGS_LEN];
        b[0] = on;
        b[1] = vb;
        b[5] = 1;
        b[8..10].copy_from_slice(&pos.to_le_bytes());
        b[160] = tb;
        b
    }

    fn reply(cmd: u8, body: &[u8]) -> Vec<u8> {
        frame(cmd | REPLY, body)
    }

    #[test]
    fn frames_match_the_captured_echo() {
        assert_eq!(frame(0x00, b"hi"), [0x53, 0x04, 0x00, 0x00, b'h', b'i', 0x28]);
        let r = [0x53, 0x04, 0x00, 0x80, b'h', b'i', 0xa8];
        assert_eq!(parse_frame(&r).unwrap(), Reply { cmd: 0x80, body: b"hi" });
    }

    #[test]
    fn bad_frames_are_rejected() {
        assert!(parse_frame(&[0x53, 0x04, 0x00, 0x80, b'h', b'i', 0x00]).is_err());
        assert!(parse_frame(&[0x54, 0x02, 0x00, 0x80, 0xd6]).is_err());
        assert!(parse_frame(&[0x53, 0x09, 0x00, 0x80, 0xd6]).is_err());
        assert!(parse_frame(&[0x53]).is_err());
    }

    #[test]
    fn settings_read_from_the_captured_blob() {
        let s = parse_settings(&settings_blob(7, -8, 20, 1)).unwrap();
        assert!(s.channels[0].on);
        assert_eq!(s.channels[0].vdiv, 2.0);
        assert_eq!(s.channels[0].position, -8);
        assert_eq!(s.tdiv, 8e-3);
        assert!(!s.channels[1].on);
    }

    #[test]
    fn probe_index_one_is_times_one() {
        assert_eq!(probe_from_index(1), 1.0);
        assert_eq!(settings_probe(1), 1.0);
    }

    fn settings_probe(i: u8) -> f64 {
        let mut b = settings_blob(7, -8, 20, 1);
        b[5] = i;
        parse_settings(&b).unwrap().channels[0].probe
    }

    #[test]
    fn twenty_volts_per_division_is_index_ten() {
        let s = parse_settings(&settings_blob(10, 0, 16, 1)).unwrap();
        assert_eq!(s.channels[0].vdiv, 20.0);
        assert_eq!(s.tdiv, 4e-4);
    }

    #[test]
    fn unknown_scale_index_is_an_error() {
        assert!(parse_settings(&settings_blob(99, 0, 20, 1)).is_err());
        assert!(parse_settings(&settings_blob(7, 0, 99, 1)).is_err());
        assert!(parse_settings(&[0; 10]).is_err());
    }

    #[test]
    fn samples_scale_by_position_and_volts_per_div() {
        let v = scale_samples(&[0xf9, 0xfa, 25, 0x00], 2.0, 1.0, -8);
        assert!((v[0] - (1.0 / 25.0 * 2.0) as f32).abs() < 1e-6);
        assert!((v[1] - (2.0 / 25.0 * 2.0) as f32).abs() < 1e-6);
        assert!((v[2] - (33.0 / 25.0 * 2.0) as f32).abs() < 1e-6);
        assert!((v[3] - (8.0 / 25.0 * 2.0) as f32).abs() < 1e-6);
    }

    #[test]
    fn rgb565_expands_to_full_range() {
        assert_eq!(rgb565_to_rgb(&[0xff, 0xff]), [255, 255, 255]);
        assert_eq!(rgb565_to_rgb(&[0x00, 0xf8]), [255, 0, 0]);
        assert_eq!(rgb565_to_rgb(&[0x1f, 0x00]), [0, 0, 255]);
    }

    #[test]
    fn capture_retries_past_the_not_ready_reply() {
        let mut replies = VecDeque::new();
        replies.push_back(reply(CMD_ECHO, b"tb"));
        replies.push_back(reply(CMD_SETTINGS, &settings_blob(7, -8, 20, 1)));
        replies.push_back(reply(CMD_SETTINGS, &settings_blob(7, -8, 20, 1)));
        replies.push_back(reply(CMD_SAMPLES, &[3, 0]));
        replies.push_back(reply(CMD_SAMPLES, &[0, 2, 0, 0]));
        replies.push_back(reply(CMD_SAMPLES, &[1, 0, 0xf9, 0xfa]));
        replies.push_back(reply(CMD_SAMPLES, &[2, 3]));
        let mut s = HantekScope::new(Fake { replies, sent: Vec::new() }).unwrap();
        let w = s.capture(0).unwrap();
        assert_eq!(w.len(), 2);
        assert_eq!(w.tdiv, 8e-3);
        assert!((w.dt - 8e-3 / 200.0).abs() < 1e-12);
        assert_eq!(w.vdiv, 2.0);
    }

    #[test]
    fn declared_length_matches_the_captured_header() {
        assert_eq!(declared_len(&[0, 0x80, 0x0c, 0]), 3200);
    }

    #[test]
    fn a_dropped_reply_is_resent() {
        let mut replies = VecDeque::new();
        replies.push_back(reply(CMD_ECHO, b"tb"));
        replies.push_back(reply(CMD_SETTINGS, &settings_blob(7, 0, 20, 1)));
        let mut s = HantekScope::new(Flaky { inner: Fake { replies, sent: Vec::new() }, drop_first: true });
        assert!(s.is_ok());
        let _ = s.as_mut().unwrap().describe();
    }

    #[test]
    fn capture_of_a_switched_off_channel_fails() {
        let mut replies = VecDeque::new();
        replies.push_back(reply(CMD_ECHO, b"tb"));
        replies.push_back(reply(CMD_SETTINGS, &settings_blob(7, 0, 20, 1)));
        replies.push_back(reply(CMD_SETTINGS, &settings_blob(7, 0, 20, 1)));
        let mut s = HantekScope::new(Fake { replies, sent: Vec::new() }).unwrap();
        assert!(s.capture(1).is_err());
    }

    #[test]
    fn png_has_the_signature() {
        let png = png_from_rgb(&[0; 2 * 2 * 3], 2, 2).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }
}
