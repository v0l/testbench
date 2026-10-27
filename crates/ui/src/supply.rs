use crate::devices::Choice;
use crate::plot::{Plot, Series as Line};
use crate::widgets::{as_limits, backend_choice, ceiling, connect_key, measured, problem, regulation_lamps, setpoint};
use egui::Ui;
use egui_bench::prelude::*;
use testbench_core::cycler_core::device::CHARGER_BACKENDS;
use testbench_core::history::{Series, value_range};
use testbench_core::limits::clamp_setpoint;
use testbench_core::worker::{SupplyCommand, SupplySession, SupplyUpdate, close_supply, spawn_supply};

pub const WINDOW_S: f64 = 120.0;

pub struct SupplyPanel {
    pub pick: Choice,
    pub session: Option<SupplySession>,
    pub last: SupplyUpdate,
    pub volts: f64,
    pub amps: f64,
    pub arm_v: f64,
    pub arm_a: f64,
    pub v_hist: Series,
    pub a_hist: Series,
    pub notices: Vec<String>,
}

impl SupplyPanel {
    pub fn new(demo: bool) -> Self {
        Self {
            pick: Choice::new(CHARGER_BACKENDS, demo),
            session: None,
            last: SupplyUpdate::default(),
            volts: 5.0,
            amps: 1.0,
            arm_v: 30.0,
            arm_a: 5.0,
            v_hist: Series::new(4000),
            a_hist: Series::new(4000),
            notices: Vec::new(),
        }
    }

    pub fn connected(&self) -> bool {
        self.session.is_some() && self.last.connected
    }

    pub fn reading(&self) -> Option<(f64, f64)> {
        self.connected().then_some((self.last.volts, self.last.amps))
    }

    pub fn connect(&mut self) {
        let Some(spec) = self.pick.spec() else {
            self.last.error = Some("nothing to open".into());
            return;
        };
        self.disconnect();
        self.last = SupplyUpdate::default();
        self.v_hist.clear();
        self.a_hist.clear();
        self.session = Some(spawn_supply(spec));
    }

    pub fn disconnect(&mut self) {
        if let Some(s) = self.session.take() {
            close_supply(s);
        }
        self.last.connected = false;
        self.last.output = None;
    }

    pub fn limits_now(&self) -> Option<testbench_core::cycler_core::device::Limits> {
        as_limits(self.last.limits)
    }

    pub fn send_setpoint(&mut self) {
        if let Some(l) = self.limits_now() {
            let (v, a) = clamp_setpoint(self.volts, self.amps, &l, self.last.armed);
            self.volts = v;
            self.amps = a;
        }
        if let Some(s) = &self.session {
            let _ = s.tx.send(SupplyCommand::Set(self.volts, self.amps));
        }
    }

    pub fn drain(&mut self, t: f64) {
        let Some(s) = &self.session else { return };
        let mut dead = false;
        while let Ok(u) = s.rx.try_recv() {
            if !u.connected {
                dead = true;
            } else {
                self.v_hist.push(t, u.volts);
                self.a_hist.push(t, u.amps);
            }
            for n in &u.notes {
                self.notices.push(n.clone());
            }
            self.last = u;
        }
        if dead && let Some(s) = self.session.take() {
            close_supply(s);
        }
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        egui::Panel::left("supply_side")
            .exact_size(340.0)
            .resizable(false)
            .frame(egui::Frame::NONE.fill(CHASSIS).inner_margin(8))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 8.0;
                    self.device_card(ui);
                    self.setpoint_card(ui);
                    self.arm_card(ui);
                });
            });
        egui::CentralPanel::no_frame().show(ui, |ui| {
            crate::widgets::chassis(ui);
            egui::Frame::NONE.inner_margin(egui::Margin::same(10)).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                self.live_card(ui);
            });
        });
    }

    fn device_card(&mut self, ui: &mut Ui) {
        let connected = self.connected();
        let rail = if connected { OK } else { ETCH };
        card(ui, Some(rail), |ui| { ui.label(legend("supply")); }, |ui| {
            if !connected {
                backend_choice(ui, "supply", &mut self.pick);
            } else {
                reading(ui, "device", self.last.name.clone());
            }
            let can = self.pick.spec().is_some();
            ui.horizontal(|ui| {
                if connect_key(ui, connected, can) {
                    if connected { self.disconnect() } else { self.connect() }
                }
                if !connected && !self.pick.demo && ui.add(Key::new("rescan").outline(VALUE)).clicked() {
                    self.pick.rescan();
                }
            });
            problem(ui, &self.last.error, &self.notices);
        });
    }

    fn setpoint_card(&mut self, ui: &mut Ui) {
        let on = self.last.output == Some(true);
        let max_v = ceiling(self.last.limits, |l| l.0, 60.0);
        let max_a = ceiling(self.last.limits, |l| l.1, 10.0);
        let enabled = self.connected();
        card(ui, Some(READOUT), |ui| { ui.label(legend("setpoint")); }, |ui| {
            ui.add_enabled_ui(enabled, |ui| {
                let (mut cv, mut ca) = (false, false);
                row(ui, "voltage", |ui| cv = setpoint(ui, &mut self.volts, max_v, 3, " V").changed());
                row(ui, "current", |ui| ca = setpoint(ui, &mut self.amps, max_a, 3, " A").changed());
                if cv || ca {
                    self.send_setpoint();
                }
                let key = if on {
                    Key::new("output off").solid(FAULT)
                } else {
                    Key::new("output on").solid(READOUT)
                };
                if ui.add(key.min_width(140.0)).clicked() {
                    self.send_setpoint();
                    if let Some(s) = &self.session {
                        let _ = s.tx.send(SupplyCommand::Output(!on));
                    }
                }
            });
        });
    }

    fn arm_card(&mut self, ui: &mut Ui) {
        let enabled = self.connected();
        let max_v = ceiling(self.last.limits, |l| l.0, 60.0);
        let max_a = ceiling(self.last.limits, |l| l.1, 10.0);
        section(ui, "arm", "hardware limits that outlive this program", |ui| {
            ui.add_enabled_ui(enabled, |ui| {
                row(ui, "max volts", |ui| { setpoint(ui, &mut self.arm_v, max_v, 2, " V"); });
                row(ui, "max amps", |ui| { setpoint(ui, &mut self.arm_a, max_a, 2, " A"); });
                if ui.add(Key::new("arm").outline(READOUT).min_width(100.0)).clicked()
                    && let Some(s) = &self.session
                {
                    let _ = s.tx.send(SupplyCommand::Arm(self.arm_v, self.arm_a));
                }
            });
            match self.last.armed {
                Some((v, a)) => status(ui, true, &format!("armed at {v:.2} V {a:.2} A")),
                None => hint(ui, "not armed: the supply may run to its own maximum"),
            }
        });
    }

    fn live_card(&mut self, ui: &mut Ui) {
        let u = &self.last;
        let live = self.connected();
        let on = u.output == Some(true);
        card(ui, Some(if live { TRACE } else { ETCH }), |ui| { ui.label(legend("output")); }, |ui| {
            let val = |x: f64, d: usize| if live { measured(x, d) } else { "-".into() };
            readouts(
                ui,
                &[
                    ("volts", val(u.volts, 3), TRACE),
                    ("amps", val(u.amps, 3), TRACE),
                    ("watts", val(u.watts(), 2), TRACE),
                ],
            );
            regulation_lamps(ui, on, u.regulation);
        });
        let t_end = self.v_hist.last().map(|(t, _)| t).unwrap_or(WINDOW_S);
        let from = t_end - WINDOW_S;
        let v = self.v_hist.window(from);
        let a = self.a_hist.window(from);
        let (lo, hi) = value_range(&[&v, &a], 1.0);
        section(ui, "history", "last two minutes", |ui| {
            Plot {
                size: egui::vec2(ui.available_width(), 220.0),
                x: (from, t_end),
                y: (lo, hi),
                series: vec![Line { points: &v, tint: TRACE }, Line { points: &a, tint: OK }],
                marker: None,
                marker_tint: READOUT,
                x_label: |x| format!("{:.0}s", x),
                y_label: |y| format!("{y:.1}"),
            }
            .show(ui);
            ui.horizontal(|ui| {
                chip(ui, "volts", TRACE, true);
                chip(ui, "amps", OK, true);
            });
        });
    }
}
