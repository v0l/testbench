use crate::plot::{Plot, Series as Line};
use crate::widgets::connect_key;
use egui::{Color32, Pos2, Sense, Stroke, StrokeKind, Ui, Vec2};
use egui_bench::prelude::*;
use std::path::PathBuf;
use testbench_core::scope::{
    ScopeCommand, ScopeSession, ScopeTarget, ScopeUpdate, close_scope, scope_targets, spawn_scope,
};
use testbench_core::screen::{DIVS_X, DIVS_Y, TDIV_STEPS, VDIV_STEPS, si, step_index, trace_points};
use testbench_core::testbench_scope::{Waveform, export, fft::spectrum, measure};

const CHANNEL_TINTS: [Color32; 4] = [READOUT, TRACE, OK, FAULT];

pub struct ScopePanel {
    demo: bool,
    pub targets: Vec<ScopeTarget>,
    pub picked: usize,
    pub session: Option<ScopeSession>,
    pub name: Option<String>,
    pub channels: usize,
    pub enabled: [bool; 4],
    pub vdiv: [usize; 4],
    pub offset: [f64; 4],
    pub tdiv: usize,
    pub trigger_channel: usize,
    pub trigger_level: f64,
    pub rising: bool,
    pub running: bool,
    pub waves: Vec<Option<Waveform>>,
    pub fft: bool,
    pub measure_channel: usize,
    pub path: String,
    pub message: Option<(bool, String)>,
}

fn default_path() -> String {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    home.join("testbench-scope.csv").to_string_lossy().into_owned()
}

impl ScopePanel {
    pub fn new(demo: bool) -> Self {
        Self {
            demo,
            targets: scope_targets(demo),
            picked: 0,
            session: None,
            name: None,
            channels: 4,
            enabled: [true, false, false, false],
            vdiv: [step_index(&VDIV_STEPS, 1.0); 4],
            offset: [0.0; 4],
            tdiv: step_index(&TDIV_STEPS, 1e-3),
            trigger_channel: 0,
            trigger_level: 0.0,
            rising: true,
            running: true,
            waves: Vec::new(),
            fft: false,
            measure_channel: 0,
            path: default_path(),
            message: None,
        }
    }

    pub fn connected(&self) -> bool {
        self.session.is_some() && self.name.is_some()
    }

    pub fn connect(&mut self) {
        let Some(t) = self.targets.get(self.picked).cloned() else {
            self.message = Some((false, "no USBTMC scope found".into()));
            return;
        };
        self.disconnect();
        self.session = Some(spawn_scope(t));
    }

    pub fn disconnect(&mut self) {
        if let Some(s) = self.session.take() {
            close_scope(s);
        }
        self.name = None;
    }

    fn send(&self, c: ScopeCommand) {
        if let Some(s) = &self.session {
            let _ = s.tx.send(c);
        }
    }

    fn push_all(&self) {
        self.send(ScopeCommand::Timebase(TDIV_STEPS[self.tdiv]));
        for c in 0..self.channels.min(4) {
            self.send(ScopeCommand::Vertical(c, VDIV_STEPS[self.vdiv[c]], self.offset[c]));
        }
        self.send(ScopeCommand::Trigger(self.trigger_channel, self.trigger_level, self.rising));
        self.send(ScopeCommand::Enable(self.enable_vec()));
    }

    fn enable_vec(&self) -> Vec<bool> {
        (0..self.channels).map(|i| i < 4 && self.enabled[i]).collect()
    }

    pub fn drain(&mut self) {
        let Some(s) = &self.session else { return };
        let mut updates = Vec::new();
        while let Ok(u) = s.rx.try_recv() {
            updates.push(u);
        }
        let mut failed_closed = false;
        for u in updates {
            match u {
                ScopeUpdate::Connected { name, channels } => {
                    self.name = Some(name);
                    self.channels = channels.min(4);
                    self.message = None;
                    self.push_all();
                }
                ScopeUpdate::Frame { waves, running } => {
                    self.waves = waves;
                    self.running = running;
                }
                ScopeUpdate::Failed(e) => {
                    failed_closed = self.name.is_none();
                    self.message = Some((false, e));
                }
            }
        }
        if failed_closed && let Some(s) = self.session.take() {
            close_scope(s);
        }
    }

    fn export(&mut self) {
        let names = ["ch1", "ch2", "ch3", "ch4"];
        let chans: Vec<(&str, &Waveform)> = self
            .waves
            .iter()
            .enumerate()
            .filter_map(|(i, w)| w.as_ref().map(|w| (names[i.min(3)], w)))
            .collect();
        if chans.is_empty() {
            self.message = Some((false, "nothing captured yet".into()));
            return;
        }
        let path = PathBuf::from(format!("{}.csv", self.path.trim_end_matches(".csv")));
        self.message = Some(match std::fs::write(&path, export::csv(&chans)) {
            Ok(()) => (true, format!("wrote {}", path.display())),
            Err(e) => (false, format!("{}: {e}", path.display())),
        });
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        egui::Panel::left("scope_side")
            .exact_size(340.0)
            .resizable(false)
            .frame(egui::Frame::NONE.fill(CHASSIS).inner_margin(8))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 8.0;
                    self.device_card(ui);
                    self.acquire_card(ui);
                    self.channel_card(ui);
                    self.export_card(ui);
                });
            });
        egui::CentralPanel::no_frame().show(ui, |ui| {
            crate::widgets::chassis(ui);
            egui::Frame::NONE.inner_margin(egui::Margin::same(10)).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                self.screen_card(ui);
                self.measure_card(ui);
            });
        });
    }

    fn device_card(&mut self, ui: &mut Ui) {
        let connected = self.connected();
        card(ui, Some(if connected { OK } else { ETCH }), |ui| { ui.label(legend("scope")); }, |ui| {
            match (&self.name, self.targets.is_empty()) {
                (Some(n), _) => reading(ui, "device", n.clone()),
                (None, true) => status(ui, false, "no USBTMC scope detected"),
                (None, false) => {
                    let labels: Vec<(usize, String)> =
                        self.targets.iter().enumerate().map(|(i, t)| (i, t.label())).collect();
                    row(ui, "device", |ui| {
                        choice(ui, "scope_dev", &mut self.picked, labels);
                    });
                }
            }
            ui.horizontal(|ui| {
                if connect_key(ui, connected, !self.targets.is_empty()) {
                    if connected { self.disconnect() } else { self.connect() }
                }
                if !connected && !self.demo && ui.add(Key::new("detect").outline(VALUE)).clicked() {
                    self.targets = scope_targets(false);
                    self.picked = self.picked.min(self.targets.len().saturating_sub(1));
                }
            });
            if let Some((ok, m)) = &self.message {
                status(ui, *ok, m);
            }
        });
    }

    fn acquire_card(&mut self, ui: &mut Ui) {
        let connected = self.connected();
        card(ui, Some(READOUT), |ui| { ui.label(legend("acquire")); }, |ui| {
            ui.add_enabled_ui(connected, |ui| {
                ui.horizontal(|ui| {
                    let run = if self.running {
                        Key::new("stop").solid(FAULT)
                    } else {
                        Key::new("run").solid(READOUT)
                    };
                    if ui.add(run.min_width(80.0)).clicked() {
                        self.send(if self.running { ScopeCommand::Stop } else { ScopeCommand::Run });
                    }
                    if ui.add(Key::new("single").outline(READOUT).min_width(80.0)).clicked() {
                        self.send(ScopeCommand::Single);
                    }
                });
                let mut changed = false;
                row(ui, "time/div", |ui| {
                    ui.allocate_ui(egui::vec2(150.0, KEY_H), |ui| {
                        changed |= choice(
                            ui,
                            "scope_tdiv",
                            &mut self.tdiv,
                            TDIV_STEPS.iter().enumerate().map(|(i, t)| (i, si(*t, "s"))),
                        );
                    });
                });
                if changed {
                    self.send(ScopeCommand::Timebase(TDIV_STEPS[self.tdiv]));
                }
                let mut trig = false;
                row(ui, "trigger", |ui| {
                    ui.allocate_ui(egui::vec2(80.0, KEY_H), |ui| {
                        trig |= choice(
                            ui,
                            "scope_trig_ch",
                            &mut self.trigger_channel,
                            (0..self.channels).map(|i| (i, format!("ch{}", i + 1))),
                        );
                    });
                    trig |= ui
                        .scope(|ui| {
                            ui.visuals_mut().override_text_color = Some(READOUT);
                            ui.add(egui::DragValue::new(&mut self.trigger_level).speed(0.01).suffix(" V"))
                        })
                        .inner
                        .changed();
                    if ui.add(Key::new(if self.rising { "rise" } else { "fall" }).outline(READOUT)).clicked() {
                        self.rising = !self.rising;
                        trig = true;
                    }
                });
                if trig {
                    self.send(ScopeCommand::Trigger(self.trigger_channel, self.trigger_level, self.rising));
                }
            });
        });
    }

    fn channel_card(&mut self, ui: &mut Ui) {
        let connected = self.connected();
        section(ui, "channels", "volts per division and offset", |ui| {
            ui.add_enabled_ui(connected, |ui| {
                for c in 0..self.channels.min(4) {
                    ui.horizontal(|ui| {
                        let mut en = self.enabled[c];
                        if toggle(ui, &format!("ch{}", c + 1), en).clicked() {
                            en = !en;
                            self.enabled[c] = en;
                            self.send(ScopeCommand::Enable(self.enable_vec()));
                        }
                        let mut changed = false;
                        ui.allocate_ui(egui::vec2(96.0, KEY_H), |ui| {
                            changed |= choice(
                                ui,
                                ("scope_vdiv", c),
                                &mut self.vdiv[c],
                                VDIV_STEPS.iter().enumerate().map(|(i, v)| (i, si(*v, "V"))),
                            );
                        });
                        changed |= ui
                            .scope(|ui| {
                                ui.visuals_mut().override_text_color = Some(READOUT);
                                ui.add(egui::DragValue::new(&mut self.offset[c]).speed(0.01).suffix(" V"))
                            })
                            .inner
                            .changed();
                        if changed {
                            self.send(ScopeCommand::Vertical(c, VDIV_STEPS[self.vdiv[c]], self.offset[c]));
                        }
                    });
                }
            });
        });
    }

    fn export_card(&mut self, ui: &mut Ui) {
        section(ui, "export", "csv of the enabled channels", |ui| {
            row(ui, "path", |ui| { field(ui, &mut self.path, "file name"); });
            if ui.add(Key::new("csv").outline(VALUE).min_width(80.0)).clicked() {
                self.export();
            }
        });
    }

    fn screen_card(&mut self, ui: &mut Ui) {
        let live = self.connected();
        let mut fft = self.fft;
        let title = if fft { "spectrum" } else { "screen" };
        card(ui, Some(if live { TRACE } else { ETCH }), |ui| {
            ui.label(legend(title));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if toggle(ui, "fft", fft).clicked() {
                    fft = !fft;
                }
            });
        }, |ui| {
            let h = (ui.available_height() - 200.0).clamp(200.0, 520.0);
            let w = ui.available_width();
            if self.fft {
                self.spectrum_view(ui, w, h);
            } else {
                self.draw_screen(ui, w, h);
            }
        });
        self.fft = fft;
    }

    fn draw_screen(&self, ui: &mut Ui, w: f32, h: f32) {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 1.0, WELL);
        p.rect_stroke(rect, 1.0, Stroke::new(1.0, ETCH), StrokeKind::Inside);
        let area = rect.shrink(6.0);
        let at = |u: (f64, f64)| {
            Pos2::new(
                area.left() + u.0 as f32 * area.width(),
                area.top() + u.1 as f32 * area.height(),
            )
        };
        for i in 0..=DIVS_X {
            let x = area.left() + area.width() * i as f32 / DIVS_X as f32;
            let tint = if i == DIVS_X / 2 { LEGEND.gamma_multiply(0.7) } else { ETCH };
            p.line_segment([Pos2::new(x, area.top()), Pos2::new(x, area.bottom())], Stroke::new(1.0, tint));
        }
        for j in 0..=DIVS_Y {
            let y = area.top() + area.height() * j as f32 / DIVS_Y as f32;
            let tint = if j == DIVS_Y / 2 { LEGEND.gamma_multiply(0.7) } else { ETCH };
            p.line_segment([Pos2::new(area.left(), y), Pos2::new(area.right(), y)], Stroke::new(1.0, tint));
        }
        let ticks = 5;
        for i in 0..=DIVS_X * ticks {
            let x = area.left() + area.width() * i as f32 / (DIVS_X * ticks) as f32;
            let cy = area.center().y;
            p.line_segment([Pos2::new(x, cy - 3.0), Pos2::new(x, cy + 3.0)], Stroke::new(1.0, ETCH));
        }
        let p = ui.painter_at(area);
        let max_points = (area.width() as usize).max(2);
        for (c, w) in self.waves.iter().enumerate() {
            let Some(w) = w else { continue };
            let pts: Vec<Pos2> = trace_points(w, w.vdiv, self.offset[c.min(3)], max_points)
                .into_iter()
                .map(at)
                .collect();
            if pts.len() > 1 {
                p.add(egui::Shape::line(pts, Stroke::new(1.5, CHANNEL_TINTS[c.min(3)])));
            }
        }
        let tdiv = self.waves.iter().flatten().next().map_or(TDIV_STEPS[self.tdiv], |w| w.tdiv);
        p.text(
            Pos2::new(rect.left() + 8.0, rect.bottom() - 4.0),
            egui::Align2::LEFT_BOTTOM,
            format!("{}/div", si(tdiv, "s")),
            mono(10.0),
            LEGEND,
        );
        for c in 0..self.channels.min(4) {
            if self.enabled[c] {
                p.text(
                    Pos2::new(rect.left() + 8.0 + 90.0 * (c as f32 + 1.0), rect.bottom() - 4.0),
                    egui::Align2::LEFT_BOTTOM,
                    format!(
                        "ch{} {}/div",
                        c + 1,
                        si(self.waves.get(c).and_then(|w| w.as_ref()).map_or(VDIV_STEPS[self.vdiv[c]], |w| w.vdiv), "V")
                    ),
                    mono(10.0),
                    CHANNEL_TINTS[c],
                );
            }
        }
    }

    fn spectrum_view(&self, ui: &mut Ui, w: f32, h: f32) {
        let Some(wave) = self.waves.get(self.measure_channel).and_then(|w| w.as_ref()) else {
            hint(ui, "enable the measured channel to see its spectrum");
            return;
        };
        let Some(sp) = spectrum(wave) else {
            hint(ui, "not enough samples for a spectrum");
            return;
        };
        let pts: Vec<(f64, f64)> =
            sp.db.iter().enumerate().skip(1).map(|(i, d)| (sp.frequency(i), f64::from(*d))).collect();
        let fmax = pts.last().map(|p| p.0).unwrap_or(1.0);
        let lo = pts.iter().map(|p| p.1).fold(f64::MAX, f64::min).max(-120.0);
        let hi = pts.iter().map(|p| p.1).fold(f64::MIN, f64::max) + 5.0;
        Plot {
            size: egui::vec2(w, h),
            x: (0.0, fmax),
            y: (lo, hi),
            series: vec![Line { points: &pts, tint: CHANNEL_TINTS[self.measure_channel.min(3)] }],
            marker: sp.peak().map(|(i, _)| sp.frequency(i)),
            marker_tint: READOUT,
            x_label: |x| si(x, "Hz"),
            y_label: |y| format!("{y:.0} dB"),
        }
        .show(ui);
    }

    fn measure_card(&mut self, ui: &mut Ui) {
        let channels: Vec<usize> =
            (0..self.channels.min(4)).filter(|&c| self.waves.get(c).is_some_and(|w| w.is_some())).collect();
        if !channels.contains(&self.measure_channel)
            && let Some(first) = channels.first()
        {
            self.measure_channel = *first;
        }
        let mut picked = self.measure_channel;
        let waves = &self.waves;
        card(ui, Some(TRACE), |ui| {
            ui.label(legend("measurements"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                for c in channels.iter().rev() {
                    if toggle(ui, &format!("ch{}", c + 1), picked == *c).clicked() {
                        picked = *c;
                    }
                }
            });
        }, |ui| {
            let m = waves.get(self.measure_channel).and_then(|w| w.as_ref()).and_then(measure);
            match m {
                Some(m) => {
                    let opt = |v: Option<f64>, u: &str| v.map(|v| si(v, u)).unwrap_or_else(|| "-".into());
                    readouts(
                        ui,
                        &[
                            ("vpp", si(m.vpp, "V"), TRACE),
                            ("vrms", si(m.vrms, "V"), TRACE),
                            ("mean", si(m.mean, "V"), TRACE),
                            ("max", si(m.vmax, "V"), TRACE),
                            ("min", si(m.vmin, "V"), TRACE),
                            ("frequency", opt(m.frequency, "Hz"), TRACE),
                            ("period", opt(m.period, "s"), TRACE),
                            ("rise", opt(m.rise_time, "s"), TRACE),
                            ("duty", m.duty.map(|d| format!("{:.1} %", d * 100.0)).unwrap_or_else(|| "-".into()), TRACE),
                        ],
                    );
                }
                None => hint(ui, "no capture yet: connect and enable a channel"),
            }
        });
        self.measure_channel = picked;
    }
}
