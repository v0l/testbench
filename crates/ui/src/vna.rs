use crate::devices::VnaChoice;
use crate::plot::{Plot, Series as Line};
use crate::smith;
use crate::widgets::{connect_key, setpoint};
use egui::Ui;
use egui_bench::prelude::*;
use std::path::PathBuf;
use testbench_core::antenna_vna::{Point, Standard};
use testbench_core::axis::{best_match, format_freq, nearest_index};
use testbench_core::export::{Z0, sweep_csv, touchstone_s1p};
use testbench_core::sweep::SweepParams;
use testbench_core::worker::{VnaCommand, VnaSession, VnaUpdate, close_vna, spawn_vna};

pub struct VnaPanel {
    pub pick: VnaChoice,
    pub session: Option<VnaSession>,
    pub name: Option<String>,
    pub range: (f64, f64),
    pub start_mhz: f64,
    pub stop_mhz: f64,
    pub points: f64,
    pub continuous: bool,
    pub use_cal: bool,
    pub cal: [bool; 3],
    pub sweep: Vec<Point>,
    pub corrected: bool,
    pub marker: Option<usize>,
    pub export_path: String,
    pub message: Option<(bool, String)>,
}

fn default_path() -> String {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    home.join("testbench-sweep").to_string_lossy().into_owned()
}

impl VnaPanel {
    pub fn new(demo: bool) -> Self {
        Self {
            pick: VnaChoice::new(demo),
            session: None,
            name: None,
            range: (50e3, 900e6),
            start_mhz: 13.0,
            stop_mhz: 15.5,
            points: 101.0,
            continuous: false,
            use_cal: true,
            cal: [false; 3],
            sweep: Vec::new(),
            corrected: false,
            marker: None,
            export_path: default_path(),
            message: None,
        }
    }

    pub fn connected(&self) -> bool {
        self.session.is_some() && self.name.is_some()
    }

    pub fn params(&self) -> SweepParams {
        SweepParams {
            start_hz: self.start_mhz * 1e6,
            stop_hz: self.stop_mhz * 1e6,
            points: self.points as usize,
        }
        .fit(self.range)
    }

    pub fn connect(&mut self) {
        let Some(t) = self.pick.selected() else {
            self.message = Some((false, "no NanoVNA found".into()));
            return;
        };
        self.disconnect();
        self.session = Some(spawn_vna(t));
    }

    pub fn disconnect(&mut self) {
        if let Some(s) = self.session.take() {
            close_vna(s);
        }
        self.name = None;
        self.continuous = false;
    }

    fn send(&self, c: VnaCommand) {
        if let Some(s) = &self.session {
            let _ = s.tx.send(c);
        }
    }

    pub fn sweep_once(&mut self) {
        let p = self.params();
        self.send(VnaCommand::Sweep(p));
    }

    pub fn drain(&mut self) {
        let Some(s) = &self.session else { return };
        let mut failed = false;
        let mut updates = Vec::new();
        while let Ok(u) = s.rx.try_recv() {
            updates.push(u);
        }
        for u in updates {
            match u {
                VnaUpdate::Connected { name, range, .. } => {
                    self.name = Some(name);
                    self.range = range;
                    self.message = None;
                }
                VnaUpdate::Sweep { points, corrected, .. } => {
                    self.sweep = points;
                    self.corrected = corrected;
                    if self.marker.is_none_or(|i| i >= self.sweep.len()) {
                        self.marker = best_match(&self.sweep);
                    }
                }
                VnaUpdate::Captured(std, flags) => {
                    self.cal = flags;
                    self.message = Some((true, format!("captured {}", std.label())));
                }
                VnaUpdate::CalCleared => self.cal = [false; 3],
                VnaUpdate::Failed(e) => {
                    self.message = Some((false, e));
                    failed = true;
                }
            }
        }
        if failed && self.name.is_none() && let Some(s) = self.session.take() {
            close_vna(s);
        }
        if failed {
            self.continuous = false;
        }
    }

    fn export(&mut self, s1p: bool) {
        if self.sweep.is_empty() {
            self.message = Some((false, "nothing to export: run a sweep first".into()));
            return;
        }
        let ext = if s1p { "s1p" } else { "csv" };
        let base = self.export_path.trim_end_matches(".csv").trim_end_matches(".s1p");
        let path = PathBuf::from(format!("{base}.{ext}"));
        let text = if s1p { touchstone_s1p(&self.sweep, Z0) } else { sweep_csv(&self.sweep, Z0) };
        self.message = Some(match std::fs::write(&path, text) {
            Ok(()) => (true, format!("wrote {}", path.display())),
            Err(e) => (false, format!("{}: {e}", path.display())),
        });
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        egui::Panel::left("vna_side")
            .exact_size(340.0)
            .resizable(false)
            .frame(egui::Frame::NONE.fill(CHASSIS).inner_margin(8))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 8.0;
                    self.device_card(ui);
                    self.sweep_card(ui);
                    self.cal_card(ui);
                    self.export_card(ui);
                });
            });
        egui::CentralPanel::no_frame().show(ui, |ui| {
            crate::widgets::chassis(ui);
            egui::Frame::NONE.inner_margin(egui::Margin::same(10)).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                self.marker_card(ui);
                self.plots(ui);
            });
        });
    }

    fn device_card(&mut self, ui: &mut Ui) {
        let connected = self.connected();
        card(ui, Some(if connected { OK } else { ETCH }), |ui| { ui.label(legend("nanovna")); }, |ui| {
            match (&self.name, self.pick.targets.is_empty()) {
                (Some(n), _) => reading(ui, "device", n.clone()),
                (None, true) => status(ui, false, "no NanoVNA detected"),
                (None, false) => {
                    let labels: Vec<(usize, String)> = self
                        .pick
                        .targets
                        .iter()
                        .enumerate()
                        .map(|(i, t)| (i, t.label()))
                        .collect();
                    row(ui, "port", |ui| {
                        choice(ui, "vna_port", &mut self.pick.picked, labels);
                    });
                }
            }
            ui.horizontal(|ui| {
                if connect_key(ui, connected, !self.pick.targets.is_empty()) {
                    if connected { self.disconnect() } else { self.connect() }
                }
                if !connected && ui.add(Key::new("detect").outline(VALUE)).clicked() {
                    self.pick.rescan();
                }
            });
            if let Some((ok, m)) = &self.message {
                status(ui, *ok, m);
            }
        });
    }

    fn sweep_card(&mut self, ui: &mut Ui) {
        let connected = self.connected();
        let max_mhz = self.range.1 / 1e6;
        card(ui, Some(READOUT), |ui| { ui.label(legend("sweep")); }, |ui| {
            ui.add_enabled_ui(connected, |ui| {
                let mut changed = false;
                row(ui, "start", |ui| changed |= setpoint(ui, &mut self.start_mhz, max_mhz, 3, " MHz").changed());
                row(ui, "stop", |ui| changed |= setpoint(ui, &mut self.stop_mhz, max_mhz, 3, " MHz").changed());
                row(ui, "points", |ui| {
                    changed |= ui
                        .scope(|ui| {
                            ui.visuals_mut().override_text_color = Some(READOUT);
                            ui.add(egui::DragValue::new(&mut self.points).range(2.0..=1001.0).speed(1.0).fixed_decimals(0))
                        })
                        .inner
                        .changed()
                });
                if changed {
                    self.send(VnaCommand::Params(self.params()));
                }
                ui.horizontal(|ui| {
                    if ui.add(Key::new("sweep once").solid(READOUT).min_width(110.0)).clicked() {
                        self.sweep_once();
                    }
                    let key = if self.continuous {
                        Key::new("stop").solid(FAULT)
                    } else {
                        Key::new("continuous").outline(READOUT)
                    };
                    if ui.add(key.min_width(110.0)).clicked() {
                        self.continuous = !self.continuous;
                        let p = self.params();
                        self.send(VnaCommand::Params(p));
                        self.send(VnaCommand::Continuous(self.continuous));
                    }
                });
            });
        });
    }

    fn cal_card(&mut self, ui: &mut Ui) {
        let connected = self.connected();
        section(ui, "calibration", "short, open, load", |ui| {
            ui.add_enabled_ui(connected, |ui| {
                hint(ui, "Connect each standard to the port, then capture it over the sweep range above. Green keys are captured.");
                ui.horizontal(|ui| {
                    for (i, std) in Standard::ALL.iter().enumerate() {
                        let key = Key::new(std.label())
                            .outline(if self.cal[i] { OK } else { READOUT });
                        if ui.add(key).clicked() {
                            let p = self.params();
                            self.send(VnaCommand::Params(p));
                            self.send(VnaCommand::Capture(*std));
                        }
                    }
                });
                if switch(ui, "apply", &mut self.use_cal, "use calibration", "Corrects sweeps taken over the same range.") {
                    self.send(VnaCommand::UseCal(self.use_cal));
                }
                if ui.add(Key::new("clear").outline(VALUE).min_width(100.0)).clicked() {
                    self.send(VnaCommand::ClearCal);
                }
            });
            if !self.sweep.is_empty() {
                status(ui, self.corrected, if self.corrected { "sweep is calibrated" } else { "sweep is raw" });
            }
        });
    }

    fn export_card(&mut self, ui: &mut Ui) {
        section(ui, "export", "csv and touchstone", |ui| {
            row(ui, "path", |ui| { field(ui, &mut self.export_path, "file name without extension"); });
            ui.horizontal(|ui| {
                if ui.add(Key::new("csv").outline(VALUE).min_width(80.0)).clicked() {
                    self.export(false);
                }
                if ui.add(Key::new("s1p").outline(VALUE).min_width(80.0)).clicked() {
                    self.export(true);
                }
            });
        });
    }

    fn marker_card(&mut self, ui: &mut Ui) {
        let pt = self.marker.and_then(|i| self.sweep.get(i)).copied();
        card(ui, Some(if pt.is_some() { TRACE } else { ETCH }), |ui| { ui.label(legend("marker")); }, |ui| {
            match pt {
                Some(p) => {
                    let z = p.z(Z0);
                    readouts(
                        ui,
                        &[
                            ("frequency", format_freq(p.freq), READOUT),
                            ("swr", format!("{:.2}", p.swr()), TRACE),
                            ("return loss", format!("{:.1} dB", p.return_loss_db()), TRACE),
                            ("impedance", format!("{:.1}{:+.1}j", z.re, z.im), TRACE),
                        ],
                    );
                    if ui.add(Key::new("best match").outline(READOUT)).clicked() {
                        self.marker = best_match(&self.sweep);
                    }
                }
                None => hint(ui, "no sweep yet: connect and sweep once"),
            }
        });
    }

    fn plots(&mut self, ui: &mut Ui) {
        if self.sweep.is_empty() {
            return;
        }
        let f0 = self.sweep.first().map(|p| p.freq).unwrap_or(0.0);
        let f1 = self.sweep.last().map(|p| p.freq).unwrap_or(1.0);
        let swr: Vec<(f64, f64)> = self.sweep.iter().map(|p| (p.freq, p.swr().min(10.0))).collect();
        let rl: Vec<(f64, f64)> = self.sweep.iter().map(|p| (p.freq, p.return_loss_db().min(40.0))).collect();
        let marker_f = self.marker.and_then(|i| self.sweep.get(i)).map(|p| p.freq);
        let freqs: Vec<f64> = self.sweep.iter().map(|p| p.freq).collect();
        let w = ui.available_width();
        let h = ((ui.available_height() - 70.0) / 2.0).clamp(90.0, 260.0);
        let mut picked = None;
        ui.horizontal_top(|ui| {
            let left = (w - 8.0 - h * 2.0 - 8.0).max(300.0);
            ui.vertical(|ui| {
                ui.set_width(left);
                ui.spacing_mut().item_spacing.y = 8.0;
                for (label, data, y, tint, fmt) in [
                    ("swr", &swr, (1.0, 10.0), TRACE, (|v| format!("{v:.1}")) as fn(f64) -> String),
                    ("return loss db", &rl, (0.0, 40.0), OK, (|v| format!("{v:.0}")) as fn(f64) -> String),
                ] {
                    ui.label(legend(label));
                    let r = Plot {
                        size: egui::vec2(left, h),
                        x: (f0, f1),
                        y,
                        series: vec![Line { points: data, tint }],
                        marker: marker_f,
                        marker_tint: READOUT,
                        x_label: |x| format!("{:.2}", x / 1e6),
                        y_label: fmt,
                    }
                    .show(ui);
                    if let Some(x) = r.clicked_x {
                        picked = nearest_index(&freqs, x);
                    }
                }
                hint(ui, "frequency axis in MHz. Click a plot to move the marker.");
            });
            ui.vertical(|ui| {
                ui.label(legend("smith chart"));
                smith::show(ui, &self.sweep, self.marker, h * 2.0);
            });
        });
        if picked.is_some() {
            self.marker = picked;
        }
    }
}
