use crate::worker::Session;
use anyhow::Result;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::Duration;
use testbench_scope::{
    DemoScope, Scope, Slope, Trigger, UsbtmcInfo, Waveform, connect_usb, list_usbtmc,
};

#[derive(Clone, Debug)]
pub enum ScopeTarget {
    Demo,
    Usb(UsbtmcInfo),
}

impl ScopeTarget {
    pub fn label(&self) -> String {
        match self {
            ScopeTarget::Demo => "demo scope".into(),
            ScopeTarget::Usb(i) => i.label(),
        }
    }
}

pub fn scope_targets(demo: bool) -> Vec<ScopeTarget> {
    if demo {
        return vec![ScopeTarget::Demo];
    }
    list_usbtmc()
        .unwrap_or_default()
        .into_iter()
        .map(ScopeTarget::Usb)
        .collect()
}

pub fn open_scope(target: &ScopeTarget) -> Result<Box<dyn Scope>> {
    match target {
        ScopeTarget::Demo => Ok(Box::new(DemoScope::new())),
        ScopeTarget::Usb(info) => connect_usb(info),
    }
}

#[derive(Debug, Clone)]
pub enum ScopeCommand {
    Run,
    Stop,
    Single,
    Timebase(f64),
    Vertical(usize, f64, f64),
    Trigger(usize, f64, bool),
    Enable(Vec<bool>),
    Quit,
}

#[derive(Debug, Clone)]
pub enum ScopeUpdate {
    Connected { name: String, channels: usize },
    Frame { waves: Vec<Option<Waveform>>, running: bool },
    Failed(String),
}

pub type ScopeSession = Session<ScopeCommand, ScopeUpdate>;

pub fn spawn_scope(target: ScopeTarget) -> ScopeSession {
    let (tx, cmd_rx) = channel::<ScopeCommand>();
    let (up_tx, rx) = channel::<ScopeUpdate>();
    let handle = std::thread::spawn(move || scope_loop(target, cmd_rx, up_tx));
    Session::from_parts(tx, rx, handle)
}

pub fn close_scope(mut s: ScopeSession) {
    let _ = s.tx.send(ScopeCommand::Quit);
    s.join();
}

fn capture_all(dev: &mut dyn Scope, enabled: &[bool]) -> Result<Vec<Option<Waveform>>> {
    enabled
        .iter()
        .enumerate()
        .map(|(i, on)| if *on { dev.capture(i).map(Some) } else { Ok(None) })
        .collect()
}

pub fn apply(dev: &mut dyn Scope, cmd: &ScopeCommand, running: &mut bool, enabled: &mut Vec<bool>) -> Result<bool> {
    let mut fresh = false;
    match cmd {
        ScopeCommand::Run => {
            dev.run()?;
            *running = true;
        }
        ScopeCommand::Stop => {
            dev.stop()?;
            *running = false;
            fresh = true;
        }
        ScopeCommand::Single => {
            dev.single()?;
            *running = false;
            fresh = true;
        }
        ScopeCommand::Timebase(t) => {
            dev.set_timebase(*t)?;
            fresh = true;
        }
        ScopeCommand::Vertical(c, v, o) => {
            dev.set_vertical(*c, *v, *o)?;
            fresh = true;
        }
        ScopeCommand::Trigger(c, level, rising) => {
            dev.set_trigger(Trigger {
                channel: *c,
                level: *level,
                slope: if *rising { Slope::Rising } else { Slope::Falling },
            })?;
        }
        ScopeCommand::Enable(e) => {
            *enabled = e.clone();
            fresh = true;
        }
        ScopeCommand::Quit => {}
    }
    Ok(fresh)
}

fn scope_loop(target: ScopeTarget, cmds: Receiver<ScopeCommand>, out: Sender<ScopeUpdate>) {
    let mut dev = match open_scope(&target) {
        Ok(d) => d,
        Err(e) => {
            let _ = out.send(ScopeUpdate::Failed(format!("{e:#}")));
            return;
        }
    };
    let channels = dev.channels();
    let d = dev.describe();
    let _ = out.send(ScopeUpdate::Connected {
        name: format!("{} {} {}", d.manufacturer, d.model, d.serial).trim().to_string(),
        channels,
    });
    let mut enabled: Vec<bool> = (0..channels).map(|i| i == 0).collect();
    let mut running = true;
    let mut fresh = true;
    loop {
        let wait = if running { Duration::from_millis(80) } else { Duration::from_millis(100) };
        match cmds.recv_timeout(wait) {
            Ok(ScopeCommand::Quit) | Err(RecvTimeoutError::Disconnected) => break,
            Ok(cmd) => match apply(dev.as_mut(), &cmd, &mut running, &mut enabled) {
                Ok(f) => fresh |= f,
                Err(e) => {
                    let _ = out.send(ScopeUpdate::Failed(format!("{e:#}")));
                }
            },
            Err(RecvTimeoutError::Timeout) => {}
        }
        if running || fresh {
            fresh = false;
            match capture_all(dev.as_mut(), &enabled) {
                Ok(waves) => {
                    if out.send(ScopeUpdate::Frame { waves, running }).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    running = false;
                    let _ = out.send(ScopeUpdate::Failed(format!("{e:#}")));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn next_frame(rx: &Receiver<ScopeUpdate>) -> (Vec<Option<Waveform>>, bool) {
        let end = Instant::now() + Duration::from_secs(5);
        loop {
            let left = end.saturating_duration_since(Instant::now());
            if let ScopeUpdate::Frame { waves, running } = rx.recv_timeout(left).expect("timeout") {
                return (waves, running);
            }
        }
    }

    #[test]
    fn demo_scope_streams_only_enabled_channels() {
        let s = spawn_scope(ScopeTarget::Demo);
        s.tx.send(ScopeCommand::Enable(vec![true, false, true, false])).unwrap();
        let mut got = next_frame(&s.rx);
        while got.0.iter().filter(|w| w.is_some()).count() != 2 {
            got = next_frame(&s.rx);
        }
        assert!(got.0[0].is_some() && got.0[1].is_none() && got.0[2].is_some());
        close_scope(s);
    }

    #[test]
    fn stop_freezes_the_picture_and_run_resumes_it() {
        let s = spawn_scope(ScopeTarget::Demo);
        s.tx.send(ScopeCommand::Stop).unwrap();
        loop {
            if !next_frame(&s.rx).1 {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(400));
        while s.rx.try_recv().is_ok() {}
        assert!(s.rx.recv_timeout(Duration::from_millis(400)).is_err());
        s.tx.send(ScopeCommand::Run).unwrap();
        assert!(next_frame(&s.rx).1);
        close_scope(s);
    }

    #[test]
    fn timebase_change_shows_up_in_the_next_frame() {
        let s = spawn_scope(ScopeTarget::Demo);
        s.tx.send(ScopeCommand::Timebase(2e-3)).unwrap();
        loop {
            let (waves, _) = next_frame(&s.rx);
            if waves[0].as_ref().is_some_and(|w| (w.tdiv - 2e-3).abs() < 1e-12) {
                break;
            }
        }
        close_scope(s);
    }
}
