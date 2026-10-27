use crate::open::{VnaTarget, open_load, open_supply, open_vna};
use crate::sweep::SweepParams;
use antenna_vna::{Calibration, Point, Standard};
use cycler_core::device::{Limits, LoadMode, LoadState, Regulation};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::{Duration, Instant};

pub const POLL: Duration = Duration::from_millis(500);

pub fn leftover_note(role: &str) -> String {
    format!("{role} was left on; output forced off")
}

pub enum SupplyCommand {
    Set(f64, f64),
    Output(bool),
    Arm(f64, f64),
    Quit,
}

#[derive(Clone, Debug, Default)]
pub struct SupplyUpdate {
    pub name: String,
    pub connected: bool,
    pub limits: Option<(f64, f64, f64)>,
    pub volts: f64,
    pub amps: f64,
    pub regulation: Option<Regulation>,
    pub output: Option<bool>,
    pub armed: Option<(f64, f64)>,
    pub notes: Vec<String>,
    pub error: Option<String>,
}

impl SupplyUpdate {
    pub fn watts(&self) -> f64 {
        self.volts * self.amps
    }

    pub fn device_limits(&self) -> Option<Limits> {
        self.limits
            .map(|(max_volts, max_amps, max_watts)| Limits { max_volts, max_amps, max_watts })
    }
}

pub struct Session<C, U> {
    pub tx: Sender<C>,
    pub rx: Receiver<U>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl<C, U> Session<C, U> {
    fn new(tx: Sender<C>, rx: Receiver<U>, handle: std::thread::JoinHandle<()>) -> Self {
        Self { tx, rx, handle: Some(handle) }
    }

    pub fn join(&mut self) {
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

pub type SupplySession = Session<SupplyCommand, SupplyUpdate>;

pub fn close_supply(mut s: SupplySession) {
    let _ = s.tx.send(SupplyCommand::Quit);
    s.join();
}

pub fn spawn_supply(spec: String) -> SupplySession {
    let (tx, cmd_rx) = channel::<SupplyCommand>();
    let (up_tx, rx) = channel::<SupplyUpdate>();
    let handle = std::thread::spawn(move || supply_loop(spec, cmd_rx, up_tx));
    Session::new(tx, rx, handle)
}

fn supply_loop(spec: String, cmds: Receiver<SupplyCommand>, out: Sender<SupplyUpdate>) {
    let mut up = SupplyUpdate::default();
    let mut dev = match open_supply(&spec) {
        Ok(d) => d,
        Err(e) => {
            up.error = Some(format!("{e:#}"));
            let _ = out.send(up);
            return;
        }
    };
    up.connected = true;
    up.name = dev.name();
    let lim = dev.limits();
    up.limits = Some((lim.max_volts, lim.max_amps, lim.max_watts));
    let mut on = dev.output_on().ok().flatten();
    if on.is_none() {
        std::thread::sleep(Duration::from_millis(300));
        on = dev.output_on().ok().flatten();
    }
    if on == Some(true) {
        let _ = dev.stop();
        up.notes.push(leftover_note("supply"));
    }
    loop {
        match cmds.recv_timeout(POLL) {
            Ok(SupplyCommand::Quit) | Err(RecvTimeoutError::Disconnected) => break,
            Ok(SupplyCommand::Set(v, a)) => {
                if let Err(e) = dev.set(v, a) {
                    up.error = Some(format!("{e:#}"));
                }
            }
            Ok(SupplyCommand::Output(want)) => {
                let r = if want { dev.start() } else { dev.stop() };
                if let Err(e) = r {
                    up.error = Some(format!("{e:#}"));
                }
            }
            Ok(SupplyCommand::Arm(v, a)) => match dev.arm(v, a) {
                Ok(true) => up.armed = Some((v, a)),
                Ok(false) => {
                    up.notes.push("supply has no hardware limits to arm".into());
                }
                Err(e) => up.error = Some(format!("{e:#}")),
            },
            Err(RecvTimeoutError::Timeout) => {}
        }
        match dev.measure() {
            Ok(s) => {
                up.volts = s.volts;
                up.amps = s.amps;
                up.error = None;
            }
            Err(e) => up.error = Some(format!("{e:#}")),
        }
        up.regulation = dev.regulation().ok().flatten();
        up.output = dev.output_on().ok().flatten();
        if out.send(up.clone()).is_err() {
            break;
        }
        up.notes.clear();
    }
    let _ = dev.stop();
}

pub enum LoadCommand {
    Set(LoadMode, f64),
    Output(bool),
    Quit,
}

#[derive(Clone, Debug, Default)]
pub struct LoadUpdate {
    pub name: String,
    pub connected: bool,
    pub limits: Option<(f64, f64, f64)>,
    pub modes: Vec<LoadMode>,
    pub state: Option<LoadState>,
    pub notes: Vec<String>,
    pub error: Option<String>,
}

impl LoadUpdate {
    pub fn device_limits(&self) -> Option<Limits> {
        self.limits
            .map(|(max_volts, max_amps, max_watts)| Limits { max_volts, max_amps, max_watts })
    }
}

pub type LoadSession = Session<LoadCommand, LoadUpdate>;

pub fn close_load(mut s: LoadSession) {
    let _ = s.tx.send(LoadCommand::Quit);
    s.join();
}

pub fn spawn_load(spec: String) -> LoadSession {
    let (tx, cmd_rx) = channel::<LoadCommand>();
    let (up_tx, rx) = channel::<LoadUpdate>();
    let handle = std::thread::spawn(move || load_loop(spec, cmd_rx, up_tx));
    Session::new(tx, rx, handle)
}

fn load_loop(spec: String, cmds: Receiver<LoadCommand>, out: Sender<LoadUpdate>) {
    let mut up = LoadUpdate::default();
    let mut dev = match open_load(&spec) {
        Ok(d) => d,
        Err(e) => {
            up.error = Some(format!("{e:#}"));
            let _ = out.send(up);
            return;
        }
    };
    up.connected = true;
    up.name = dev.name();
    up.modes = dev.modes().to_vec();
    let lim = dev.limits();
    up.limits = Some((lim.max_volts, lim.max_amps, lim.max_watts));
    let mut on = dev.output_on().ok().flatten();
    if on.is_none() {
        std::thread::sleep(Duration::from_millis(300));
        on = dev.output_on().ok().flatten();
    }
    if on == Some(true) {
        let _ = dev.stop();
        up.notes.push(leftover_note("load"));
    }
    loop {
        match cmds.recv_timeout(POLL) {
            Ok(LoadCommand::Quit) | Err(RecvTimeoutError::Disconnected) => break,
            Ok(LoadCommand::Set(mode, v)) => {
                if let Err(e) = dev.set_mode(mode, v) {
                    up.error = Some(format!("{e:#}"));
                }
            }
            Ok(LoadCommand::Output(want)) => {
                let r = if want { dev.start() } else { dev.stop() };
                if let Err(e) = r {
                    up.error = Some(format!("{e:#}"));
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
        match dev.state() {
            Ok(s) => {
                up.state = Some(s);
                up.error = None;
            }
            Err(e) => up.error = Some(format!("{e:#}")),
        }
        if out.send(up.clone()).is_err() {
            break;
        }
        up.notes.clear();
    }
    let _ = dev.stop();
}

pub enum VnaCommand {
    Sweep(SweepParams),
    Continuous(bool),
    Params(SweepParams),
    Capture(Standard),
    ClearCal,
    UseCal(bool),
    Quit,
}

#[derive(Clone, Debug)]
pub enum VnaUpdate {
    Connected { name: String, range: (f64, f64), max_points: usize },
    Sweep { points: Vec<Point>, corrected: bool, at: Instant },
    Captured(Standard, [bool; 3]),
    CalCleared,
    Failed(String),
}

pub type VnaSession = Session<VnaCommand, VnaUpdate>;

pub fn close_vna(mut s: VnaSession) {
    let _ = s.tx.send(VnaCommand::Quit);
    s.join();
}

pub fn spawn_vna(target: VnaTarget) -> VnaSession {
    let (tx, cmd_rx) = channel::<VnaCommand>();
    let (up_tx, rx) = channel::<VnaUpdate>();
    let handle = std::thread::spawn(move || vna_loop(target, cmd_rx, up_tx));
    Session::new(tx, rx, handle)
}

pub fn cal_flags(cal: &Calibration) -> [bool; 3] {
    [cal.has(Standard::Open), cal.has(Standard::Short), cal.has(Standard::Load)]
}

fn vna_loop(target: VnaTarget, cmds: Receiver<VnaCommand>, out: Sender<VnaUpdate>) {
    let mut dev = match open_vna(&target) {
        Ok(d) => d,
        Err(e) => {
            let _ = out.send(VnaUpdate::Failed(format!("{e:#}")));
            return;
        }
    };
    let _ = out.send(VnaUpdate::Connected {
        name: dev.describe(),
        range: dev.range_hz(),
        max_points: dev.max_points(),
    });
    let mut params: Option<SweepParams> = None;
    let mut continuous = false;
    let mut use_cal = true;
    let mut cal = Calibration::default();
    let mut pending: Option<VnaCommand> = None;
    loop {
        let cmd = match pending.take() {
            Some(c) => Some(c),
            None => {
                let wait = if continuous { Duration::from_millis(1) } else { Duration::from_millis(100) };
                match cmds.recv_timeout(wait) {
                    Ok(c) => Some(c),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
        };
        let mut do_sweep = continuous && cmd.is_none();
        let mut capture = None;
        match cmd {
            Some(VnaCommand::Quit) => break,
            Some(VnaCommand::Params(p)) => params = Some(p),
            Some(VnaCommand::Sweep(p)) => {
                params = Some(p);
                do_sweep = true;
            }
            Some(VnaCommand::Continuous(on)) => continuous = on,
            Some(VnaCommand::Capture(s)) => capture = Some(s),
            Some(VnaCommand::ClearCal) => {
                cal = Calibration::default();
                let _ = out.send(VnaUpdate::CalCleared);
            }
            Some(VnaCommand::UseCal(on)) => use_cal = on,
            None => {}
        }
        let Some(p) = params else { continue };
        if !(do_sweep || capture.is_some()) {
            continue;
        }
        match dev.sweep(p.start_hz, p.stop_hz, p.points, false) {
            Ok(points) => {
                if let Some(std) = capture {
                    cal.store(std, &points);
                    let _ = out.send(VnaUpdate::Captured(std, cal_flags(&cal)));
                } else {
                    let corrected = use_cal && cal.matches(&points);
                    let points = if corrected { cal.apply(&points) } else { points };
                    if out.send(VnaUpdate::Sweep { points, corrected, at: Instant::now() }).is_err() {
                        break;
                    }
                }
            }
            Err(e) => {
                continuous = false;
                let _ = out.send(VnaUpdate::Failed(format!("{e}")));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::open::{DEMO_ON_SPEC, DEMO_SPEC};

    fn wait_for<T>(rx: &Receiver<T>, mut pick: impl FnMut(&T) -> bool) -> T {
        let end = Instant::now() + Duration::from_secs(5);
        loop {
            let left = end.saturating_duration_since(Instant::now());
            let v = rx.recv_timeout(left).expect("timed out");
            if pick(&v) {
                return v;
            }
        }
    }

    #[test]
    fn a_supply_found_on_is_forced_off_and_reported() {
        let s = spawn_supply(DEMO_ON_SPEC.into());
        let first = wait_for(&s.rx, |u| u.connected);
        assert_eq!(first.notes, vec![leftover_note("supply")]);
        assert_eq!(first.output, Some(false));
        close_supply(s);
    }

    #[test]
    fn a_load_found_on_is_forced_off_and_reported() {
        let s = spawn_load(DEMO_ON_SPEC.into());
        let first = wait_for(&s.rx, |u| u.connected);
        assert_eq!(first.notes, vec![leftover_note("load")]);
        assert!(!first.state.unwrap().on);
        close_load(s);
    }

    #[test]
    fn supply_worker_applies_setpoints_and_reports_regulation() {
        let s = spawn_supply(DEMO_SPEC.into());
        s.tx.send(SupplyCommand::Set(12.0, 1.0)).unwrap();
        s.tx.send(SupplyCommand::Output(true)).unwrap();
        let u = wait_for(&s.rx, |u| u.output == Some(true));
        assert_eq!(u.regulation, Some(Regulation::Cc));
        assert!((u.amps - 1.0).abs() < 0.05);
        close_supply(s);
    }

    #[test]
    fn load_worker_switches_mode_and_integrates() {
        let s = spawn_load(DEMO_SPEC.into());
        let first = wait_for(&s.rx, |u| u.connected);
        assert!(first.modes.contains(&LoadMode::Cr));
        s.tx.send(LoadCommand::Set(LoadMode::Cr, 5.0)).unwrap();
        s.tx.send(LoadCommand::Output(true)).unwrap();
        let u = wait_for(&s.rx, |u| u.state.is_some_and(|s| s.on));
        let st = u.state.unwrap();
        assert!((st.volts / st.amps - 5.0).abs() < 0.1);
        close_load(s);
    }

    #[test]
    fn vna_worker_sweeps_and_applies_a_complete_calibration() {
        let s = spawn_vna(VnaTarget::Demo);
        wait_for(&s.rx, |u| matches!(u, VnaUpdate::Connected { .. }));
        let p = SweepParams { start_hz: 13e6, stop_hz: 15e6, points: 21 };
        s.tx.send(VnaCommand::Params(p)).unwrap();
        s.tx.send(VnaCommand::Sweep(p)).unwrap();
        let first = wait_for(&s.rx, |u| matches!(u, VnaUpdate::Sweep { .. }));
        let VnaUpdate::Sweep { corrected, .. } = first else { unreachable!() };
        assert!(!corrected);
        for std in Standard::ALL {
            s.tx.send(VnaCommand::Capture(std)).unwrap();
            wait_for(&s.rx, |u| matches!(u, VnaUpdate::Captured(x, _) if *x == std));
        }
        s.tx.send(VnaCommand::Sweep(p)).unwrap();
        let again = wait_for(&s.rx, |u| matches!(u, VnaUpdate::Sweep { .. }));
        let VnaUpdate::Sweep { corrected, .. } = again else { unreachable!() };
        assert!(corrected);
        close_vna(s);
    }
}
