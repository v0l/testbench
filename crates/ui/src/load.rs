use crate::devices::Choice;
use crate::plot::{Plot, Series as Line};
use crate::widgets::{as_limits, backend_choice, ceiling, connect_key, measured, problem, setpoint};
use egui::Ui;
use egui_bench::prelude::*;
use testbench_core::cycler_core::device::{LOAD_BACKENDS, LoadMode};
use testbench_core::history::{Series, value_range};
use testbench_core::limits::clamp_load;
use testbench_core::worker::{LoadCommand, LoadSession, LoadUpdate, close_load, spawn_load};

pub const WINDOW_S: f64 = 120.0;

pub struct LoadPanel {
    pub pick: Choice,
    pub session: Option<LoadSession>,
    pub last: LoadUpdate,
    pub mode: LoadMode,
    pub value: f64,
    pub v_hist: Series,
    pub a_hist: Series,
    pub notices: Vec<String>,
}

impl LoadPanel {
    pub fn new(demo: bool) -> Self {
        Self {
            pick: Choice::new(LOAD_BACKENDS, demo),
            session: None,
            last: LoadUpdate::default(),
            mode: LoadMode::Cc,
            value: 0.5,
            v_hist: Series::new(4000),
            a_hist: Series::new(4000),
            notices: Vec::new(),
        }
    }

    pub fn connected(&self) -> bool {
        self.session.is_some() && self.last.connected
    }

    pub fn reading(&self) -> Option<(f64, f64)> {
        let s = self.last.state?;
        self.connected().then_some((s.volts, s.amps))
    }

    pub fn connect(&mut self) {
        let Some(spec) = self.pick.spec() else {
            self.last.error = Some("nothing to open".into());
            return;
        };
        self.disconnect();
        self.last = LoadUpdate::default();
        self.v_hist.clear();
        self.a_hist.clear();
        self.session = Some(spawn_load(spec));
    }

    pub fn disconnect(&mut self) {
        if let Some(s) = self.session.take() {
            close_load(s);
        }
        self.last.connected = false;
        self.last.state = None;
    }

    pub fn send_setpoint(&mut self) {
        if let Some(l) = as_limits(self.last.limits) {
            self.value = clamp_load(self.mode, self.value, &l);
        }
        if let Some(s) = &self.session {
            let _ = s.tx.send(LoadCommand::Set(self.mode, self.value));
        }
    }

    pub fn drain(&mut self, t: f64) {
        let Some(s) = &self.session else { return };
        let mut dead = false;
        while let Ok(u) = s.rx.try_recv() {
            if !u.connected {
                dead = true;
            } else if let Some(st) = &u.state {
                self.v_hist.push(t, st.volts);
                self.a_hist.push(t, st.amps);
            }
            for n in &u.notes {
                self.notices.push(n.clone());
            }
            if u.connected
                && !self.last.connected
                && let Some(m) = u.modes.first()
            {
                self.mode = *m;
            }
            self.last = u;
        }
        if dead && let Some(s) = self.session.take() {
            close_load(s);
        }
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        egui::Panel::left("load_side")
            .exact_size(340.0)
            .resizable(false)
            .frame(egui::Frame::NONE.fill(CHASSIS).inner_margin(8))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 8.0;
                    self.device_card(ui);
                    self.setpoint_card(ui);
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
        card(ui, Some(if connected { OK } else { ETCH }), |ui| { ui.label(legend("load")); }, |ui| {
            if !connected {
                backend_choice(ui, "load", &mut self.pick);
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
        let on = self.last.state.is_some_and(|s| s.on);
        let enabled = self.connected();
        let modes: Vec<LoadMode> = if self.last.modes.is_empty() { vec![LoadMode::Cc] } else { self.last.modes.clone() };
        let max = match self.mode {
            LoadMode::Cc => ceiling(self.last.limits, |l| l.1, 30.0),
            LoadMode::Cv => ceiling(self.last.limits, |l| l.0, 60.0),
            LoadMode::Cp => ceiling(self.last.limits, |l| l.2, 300.0),
            LoadMode::Cr => 10_000.0,
        };
        card(ui, Some(READOUT), |ui| { ui.label(legend("setpoint")); }, |ui| {
            ui.add_enabled_ui(enabled, |ui| {
                let mut changed = false;
                row(ui, "mode", |ui| {
                    ui.allocate_ui(egui::vec2(120.0, KEY_H), |ui| {
                        changed |= choice(ui, "load_mode", &mut self.mode, modes.iter().map(|m| (*m, m.label().to_string())));
                    });
                });
                let unit = format!(" {}", self.mode.unit());
                row(ui, "value", |ui| changed |= setpoint(ui, &mut self.value, max, 3, &unit).changed());
                if changed {
                    self.send_setpoint();
                }
                let key = if on {
                    Key::new("load off").solid(FAULT)
                } else {
                    Key::new("load on").solid(READOUT)
                };
                if ui.add(key.min_width(140.0)).clicked() {
                    self.send_setpoint();
                    if let Some(s) = &self.session {
                        let _ = s.tx.send(LoadCommand::Output(!on));
                    }
                }
            });
        });
    }

    fn live_card(&mut self, ui: &mut Ui) {
        let live = self.connected();
        let st = self.last.state.unwrap_or_default();
        card(ui, Some(if live { TRACE } else { ETCH }), |ui| { ui.label(legend("input")); }, |ui| {
            let val = |x: f64, d: usize| if live { measured(x, d) } else { "-".into() };
            readouts(
                ui,
                &[
                    ("volts", val(st.volts, 3), TRACE),
                    ("amps", val(st.amps, 3), TRACE),
                    ("watts", val(st.watts, 2), TRACE),
                    ("amp hours", val(st.amp_hours, 4), TRACE),
                    ("watt hours", val(st.watt_hours, 3), TRACE),
                    ("temp", if live { format!("{:.1} C", st.temp_c) } else { "-".into() }, TRACE),
                ],
            );
            ui.horizontal(|ui| {
                lamp(ui, "input", st.on, false);
                if live {
                    lamp(ui, self.mode.label(), st.on, false);
                }
            });
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
