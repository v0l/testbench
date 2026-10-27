use crate::plot::{Plot, Series as Line};
use egui::Ui;
use egui_bench::prelude::*;
use std::path::PathBuf;
use testbench_core::csvlog::Recorder;
use testbench_core::history::{Series, time_range, value_range};

pub const SPAN_S: f64 = 120.0;
pub const SAMPLE_EVERY_S: f64 = 0.5;

pub struct LogPanel {
    pub path: String,
    pub recorder: Option<Recorder<std::io::BufWriter<std::fs::File>>>,
    pub message: Option<(bool, String)>,
    last_sample: f64,
}

fn default_path() -> String {
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    home.join(format!("testbench-{stamp}.csv")).to_string_lossy().into_owned()
}

impl LogPanel {
    pub fn new() -> Self {
        Self { path: default_path(), recorder: None, message: None, last_sample: f64::NEG_INFINITY }
    }

    pub fn recording(&self) -> bool {
        self.recorder.is_some()
    }

    pub fn start(&mut self) {
        self.message = Some(match Recorder::create(&self.path) {
            Ok(r) => {
                self.recorder = Some(r);
                (true, format!("recording to {}", self.path))
            }
            Err(e) => (false, format!("{}: {e:#}", self.path)),
        });
    }

    pub fn stop(&mut self) {
        if let Some(r) = self.recorder.take() {
            self.message = Some((true, format!("saved {} rows to {}", r.rows(), r.path().display())));
        }
    }

    pub fn sample(&mut self, t: f64, supply: Option<(f64, f64)>, load: Option<(f64, f64)>) {
        if t - self.last_sample < SAMPLE_EVERY_S {
            return;
        }
        let Some(r) = self.recorder.as_mut() else { return };
        self.last_sample = t;
        if let Err(e) = r.record(supply, load) {
            self.message = Some((false, format!("{e:#}")));
            self.recorder = None;
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, hist: [&Series; 4]) {
        egui::CentralPanel::no_frame().show(ui, |ui| {
            crate::widgets::chassis(ui);
            egui::Frame::NONE.inner_margin(egui::Margin::same(10)).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                self.record_card(ui);
                self.combined(ui, hist);
            });
        });
    }

    fn record_card(&mut self, ui: &mut Ui) {
        let rec = self.recording();
        card(ui, Some(if rec { FAULT } else { ETCH }), |ui| { ui.label(legend("record")); }, |ui| {
            row(ui, "file", |ui| {
                ui.add_enabled_ui(!rec, |ui| { field(ui, &mut self.path, "csv file"); });
            });
            ui.horizontal(|ui| {
                if rec {
                    if ui.add(Key::new("stop recording").solid(FAULT).min_width(140.0)).clicked() {
                        self.stop();
                    }
                    if let Some(r) = &self.recorder {
                        egui_bench::prelude::Line::new().legend("rows").measured(r.rows().to_string()).show(ui);
                    }
                } else if ui.add(Key::new("record").solid(READOUT).min_width(140.0)).clicked() {
                    self.start();
                }
            });
            if let Some((ok, m)) = &self.message {
                status(ui, *ok, m);
            }
            hint(ui, "Samples both devices twice a second. Columns: timestamp, supply and load volts, amps, watts.");
        });
    }

    fn combined(&mut self, ui: &mut Ui, [sv, sa, lv, la]: [&Series; 4]) {
        let end = [sv, sa, lv, la].iter().filter_map(|s| s.last()).map(|(t, _)| t).fold(0.0, f64::max);
        let from = end - SPAN_S;
        let w: [Vec<(f64, f64)>; 4] = [sv.window(from), sa.window(from), lv.window(from), la.window(from)];
        let (xlo, xhi) = time_range(&[&w[0], &w[1], &w[2], &w[3]], SPAN_S);
        let (vlo, vhi) = value_range(&[&w[0], &w[2]], 1.0);
        let (alo, ahi) = value_range(&[&w[1], &w[3]], 1.0);
        section(ui, "combined", "supply and load on one time axis", |ui| {
            let h = ((ui.available_height() - 60.0) / 2.0).clamp(120.0, 300.0);
            ui.label(legend("volts"));
            Plot {
                size: egui::vec2(ui.available_width(), h),
                x: (xlo, xhi),
                y: (vlo, vhi),
                series: vec![Line { points: &w[0], tint: READOUT }, Line { points: &w[2], tint: TRACE }],
                marker: None,
                marker_tint: READOUT,
                x_label: |x| format!("{x:.0}s"),
                y_label: |y| format!("{y:.1}"),
            }
            .show(ui);
            ui.label(legend("amps"));
            Plot {
                size: egui::vec2(ui.available_width(), h),
                x: (xlo, xhi),
                y: (alo, ahi),
                series: vec![Line { points: &w[1], tint: READOUT }, Line { points: &w[3], tint: TRACE }],
                marker: None,
                marker_tint: READOUT,
                x_label: |x| format!("{x:.0}s"),
                y_label: |y| format!("{y:.2}"),
            }
            .show(ui);
            ui.horizontal(|ui| {
                chip(ui, "supply", READOUT, true);
                chip(ui, "load", TRACE, true);
            });
        });
    }
}
