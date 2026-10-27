use anyhow::{Result, bail, ensure};

pub const HEADER_LEN: usize = 12;
pub const MSG_DEV_DEP_MSG_OUT: u8 = 1;
pub const MSG_REQUEST_DEV_DEP_MSG_IN: u8 = 2;
pub const MSG_DEV_DEP_MSG_IN: u8 = 2;
pub const CLASS_APPLICATION: u8 = 0xFE;
pub const SUBCLASS_USBTMC: u8 = 3;
pub const READ_REQUEST: u32 = 1 << 20;
pub const IN_TRANSFER: usize = 1 << 16;

pub fn next_tag(tag: u8) -> u8 {
    match tag.wrapping_add(1) {
        0 => 1,
        t => t,
    }
}

pub fn pad_to_four(len: usize) -> usize {
    (4 - len % 4) % 4
}

pub fn build_dev_dep_msg_out(tag: u8, data: &[u8], eom: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + data.len() + 3);
    out.extend_from_slice(&[MSG_DEV_DEP_MSG_OUT, tag, !tag, 0]);
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&[u8::from(eom), 0, 0, 0]);
    out.extend_from_slice(data);
    out.resize(out.len() + pad_to_four(data.len()), 0);
    out
}

pub fn build_request_dev_dep_msg_in(tag: u8, size: u32, term_char: Option<u8>) -> [u8; HEADER_LEN] {
    let mut out = [0u8; HEADER_LEN];
    out[0] = MSG_REQUEST_DEV_DEP_MSG_IN;
    out[1] = tag;
    out[2] = !tag;
    out[4..8].copy_from_slice(&size.to_le_bytes());
    if let Some(c) = term_char {
        out[8] = 2;
        out[9] = c;
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InHeader {
    pub tag: u8,
    pub transfer_size: usize,
    pub eom: bool,
}

pub fn parse_in_header(buf: &[u8]) -> Result<InHeader> {
    ensure!(buf.len() >= HEADER_LEN, "short USBTMC header: {} bytes", buf.len());
    ensure!(
        buf[0] == MSG_DEV_DEP_MSG_IN,
        "unexpected USBTMC message id {}",
        buf[0]
    );
    ensure!(buf[1] == !buf[2], "USBTMC bTag and its inverse disagree");
    Ok(InHeader {
        tag: buf[1],
        transfer_size: u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]) as usize,
        eom: buf[8] & 1 != 0,
    })
}

pub fn message_len(header: &InHeader) -> usize {
    HEADER_LEN + header.transfer_size
}

pub trait Pipe: Send {
    fn write(&mut self, data: &[u8]) -> Result<()>;
    fn read(&mut self) -> Result<Vec<u8>>;
}

pub struct UsbTmc<P: Pipe> {
    pipe: P,
    tag: u8,
}

impl<P: Pipe> UsbTmc<P> {
    pub fn new(pipe: P) -> Self {
        Self { pipe, tag: 0 }
    }

    fn bump(&mut self) -> u8 {
        self.tag = next_tag(self.tag);
        self.tag
    }

    pub fn send(&mut self, data: &[u8]) -> Result<()> {
        let tag = self.bump();
        let packet = build_dev_dep_msg_out(tag, data, true);
        self.pipe.write(&packet)
    }

    pub fn receive(&mut self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        loop {
            let tag = self.bump();
            let request = build_request_dev_dep_msg_in(tag, READ_REQUEST, None);
            self.pipe.write(&request)?;
            let mut buf = self.pipe.read()?;
            let header = parse_in_header(&buf)?;
            if header.tag != tag {
                bail!("USBTMC reply tag {} does not match request {}", header.tag, tag);
            }
            let want = message_len(&header);
            while buf.len() < want {
                let more = self.pipe.read()?;
                ensure!(!more.is_empty(), "USBTMC transfer ended early");
                buf.extend_from_slice(&more);
            }
            out.extend_from_slice(&buf[HEADER_LEN..want]);
            if header.eom {
                return Ok(out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    #[test]
    fn out_message_is_padded_and_flagged() {
        let p = build_dev_dep_msg_out(7, b"*IDN?", true);
        assert_eq!(p.len() % 4, 0);
        assert_eq!(p.len(), 12 + 8);
        assert_eq!(&p[..4], &[1, 7, !7u8, 0]);
        assert_eq!(u32::from_le_bytes([p[4], p[5], p[6], p[7]]), 5);
        assert_eq!(p[8], 1);
        assert_eq!(&p[12..17], b"*IDN?");
        assert_eq!(&p[17..], &[0, 0, 0]);
    }

    #[test]
    fn out_message_without_eom_and_aligned_payload() {
        let p = build_dev_dep_msg_out(1, b"abcd", false);
        assert_eq!(p.len(), 16);
        assert_eq!(p[8], 0);
    }

    #[test]
    fn request_header_layout() {
        let r = build_request_dev_dep_msg_in(9, 4096, None);
        assert_eq!(&r[..4], &[2, 9, !9u8, 0]);
        assert_eq!(u32::from_le_bytes([r[4], r[5], r[6], r[7]]), 4096);
        assert_eq!(r[8], 0);
        let t = build_request_dev_dep_msg_in(9, 16, Some(b'\n'));
        assert_eq!(t[8], 2);
        assert_eq!(t[9], b'\n');
    }

    #[test]
    fn tag_never_zero() {
        assert_eq!(next_tag(255), 1);
        assert_eq!(next_tag(1), 2);
        assert_eq!(next_tag(0), 1);
    }

    #[test]
    fn in_header_parses_and_validates() {
        let mut h = vec![2, 3, !3u8, 0, 10, 0, 0, 0, 1, 0, 0, 0];
        let p = parse_in_header(&h).unwrap();
        assert_eq!(p, InHeader { tag: 3, transfer_size: 10, eom: true });
        assert_eq!(message_len(&p), 22);
        h[2] = 0;
        assert!(parse_in_header(&h).is_err());
        assert!(parse_in_header(&h[..8]).is_err());
        let mut wrong = vec![1, 3, !3u8, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        assert!(parse_in_header(&wrong).is_err());
        wrong[0] = 2;
        assert!(parse_in_header(&wrong).is_ok());
    }

    struct Mock {
        written: Vec<Vec<u8>>,
        replies: VecDeque<Vec<u8>>,
    }

    impl Pipe for Mock {
        fn write(&mut self, data: &[u8]) -> Result<()> {
            self.written.push(data.to_vec());
            Ok(())
        }
        fn read(&mut self) -> Result<Vec<u8>> {
            self.replies.pop_front().ok_or_else(|| anyhow::anyhow!("no reply"))
        }
    }

    fn reply(tag: u8, payload: &[u8], eom: bool) -> Vec<u8> {
        let mut v = vec![2, tag, !tag, 0];
        v.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        v.extend_from_slice(&[u8::from(eom), 0, 0, 0]);
        v.extend_from_slice(payload);
        v.resize(v.len() + pad_to_four(payload.len()), 0);
        v
    }

    #[test]
    fn receive_joins_split_transfers() {
        let full = reply(2, b"hello world", true);
        let (a, b) = full.split_at(15);
        let mut t = UsbTmc::new(Mock {
            written: vec![],
            replies: VecDeque::from(vec![a.to_vec(), b.to_vec()]),
        });
        t.send(b"X").unwrap();
        let got = t.receive().unwrap();
        assert_eq!(got, b"hello world");
        assert_eq!(t.pipe.written.len(), 2);
        assert_eq!(t.pipe.written[1][0], MSG_REQUEST_DEV_DEP_MSG_IN);
    }

    #[test]
    fn receive_requests_again_until_eom() {
        let mut t = UsbTmc::new(Mock {
            written: vec![],
            replies: VecDeque::from(vec![reply(1, b"abc", false), reply(2, b"def", true)]),
        });
        assert_eq!(t.receive().unwrap(), b"abcdef");
        assert_eq!(t.pipe.written.len(), 2);
    }

    #[test]
    fn receive_rejects_wrong_tag() {
        let mut t = UsbTmc::new(Mock {
            written: vec![],
            replies: VecDeque::from(vec![reply(9, b"abc", true)]),
        });
        assert!(t.receive().is_err());
    }
}
