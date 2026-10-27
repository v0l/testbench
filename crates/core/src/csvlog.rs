use anyhow::Result;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

pub const LOG_HEADER: &str = "timestamp,supply_v,supply_a,supply_w,load_v,load_a,load_w";

fn cells(reading: Option<(f64, f64)>) -> String {
    match reading {
        Some((v, a)) => format!("{v:.4},{a:.4},{:.4}", v * a),
        None => ",,".to_string(),
    }
}

pub fn log_row(ts: &str, supply: Option<(f64, f64)>, load: Option<(f64, f64)>) -> String {
    format!("{ts},{},{}\n", cells(supply), cells(load))
}

pub struct Recorder<W: Write> {
    out: W,
    rows: usize,
    path: PathBuf,
}

impl Recorder<BufWriter<File>> {
    pub fn create(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)?;
        }
        let file = BufWriter::new(File::create(&path)?);
        Recorder::new(file, path)
    }

    pub fn record(&mut self, supply: Option<(f64, f64)>, load: Option<(f64, f64)>) -> Result<()> {
        let ts = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%.3f").to_string();
        self.record_at(&ts, supply, load)
    }
}

impl<W: Write> Recorder<W> {
    pub fn new(mut out: W, path: PathBuf) -> Result<Self> {
        writeln!(out, "{LOG_HEADER}")?;
        Ok(Self { out, rows: 0, path })
    }

    pub fn record_at(
        &mut self,
        ts: &str,
        supply: Option<(f64, f64)>,
        load: Option<(f64, f64)>,
    ) -> Result<()> {
        self.out.write_all(log_row(ts, supply, load).as_bytes())?;
        self.out.flush()?;
        self.rows += 1;
        Ok(())
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn into_inner(self) -> W {
        self.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_carry_watts_and_leave_missing_devices_blank() {
        assert_eq!(
            log_row("t0", Some((12.0, 0.5)), None),
            "t0,12.0000,0.5000,6.0000,,,\n"
        );
        assert_eq!(
            log_row("t1", None, Some((3.0, 2.0))),
            "t1,,,,3.0000,2.0000,6.0000\n"
        );
    }

    #[test]
    fn recorder_writes_header_once_then_rows() {
        let mut r = Recorder::new(Vec::new(), PathBuf::from("x.csv")).unwrap();
        r.record_at("a", Some((1.0, 1.0)), Some((2.0, 2.0))).unwrap();
        r.record_at("b", None, None).unwrap();
        assert_eq!(r.rows(), 2);
        let text = String::from_utf8(r.into_inner()).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], LOG_HEADER);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[1], "a,1.0000,1.0000,1.0000,2.0000,2.0000,4.0000");
    }

    #[test]
    fn create_makes_the_file_and_stamps_rows() {
        let dir = std::env::temp_dir().join(format!("testbench-log-{}", std::process::id()));
        let path = dir.join("nested").join("log.csv");
        let mut r = Recorder::create(&path).unwrap();
        r.record(Some((5.0, 1.0)), None).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), 2);
        assert!(text.lines().nth(1).unwrap().contains('T'));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
