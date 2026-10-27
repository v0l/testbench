use egui::{DragValue, Response, Ui};
use egui_bench::prelude::{Key, READOUT, choice, lamp, row, status};
use testbench_core::cycler_core::device::{Limits, Regulation};

pub fn setpoint(ui: &mut Ui, value: &mut f64, max: f64, decimals: usize, suffix: &str) -> Response {
    ui.scope(|ui| {
        ui.visuals_mut().override_text_color = Some(READOUT);
        ui.add(
            DragValue::new(value)
                .range(0.0..=max)
                .speed(10f64.powi(-(decimals as i32)))
                .fixed_decimals(decimals)
                .suffix(suffix),
        )
    })
    .inner
}

pub fn ceiling(limits: Option<(f64, f64, f64)>, pick: impl Fn(&(f64, f64, f64)) -> f64, fallback: f64) -> f64 {
    limits.as_ref().map(&pick).filter(|m| *m > 0.0).unwrap_or(fallback)
}

pub fn as_limits(l: Option<(f64, f64, f64)>) -> Option<Limits> {
    l.map(|(max_volts, max_amps, max_watts)| Limits { max_volts, max_amps, max_watts })
}

pub fn regulation_lamps(ui: &mut Ui, on: bool, reg: Option<Regulation>) {
    ui.horizontal(|ui| {
        lamp(ui, "output", on, false);
        lamp(ui, "cv", on && reg == Some(Regulation::Cv), false);
        lamp(ui, "cc", on && reg == Some(Regulation::Cc), false);
        lamp(ui, "fault", false, reg == Some(Regulation::Fault));
    });
}

pub fn connect_key(ui: &mut Ui, connected: bool, can: bool) -> bool {
    let key = if connected {
        Key::new("disconnect").outline(egui_bench::prelude::VALUE)
    } else {
        Key::new("connect").solid(READOUT).enabled(can)
    };
    ui.add(key.min_width(110.0)).clicked()
}

pub fn pick_row(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui)) {
    row(ui, label, add);
}

pub fn problem(ui: &mut Ui, error: &Option<String>, notes: &[String]) {
    if let Some(e) = error {
        status(ui, false, e);
    }
    for n in notes.iter().rev().take(2) {
        status(ui, false, n);
    }
}

pub fn measured(v: f64, decimals: usize) -> String {
    format!("{v:.decimals$}")
}

pub fn backend_choice(ui: &mut Ui, id: &str, c: &mut crate::devices::Choice) -> bool {
    let mut changed = false;
    if c.demo {
        egui_bench::prelude::reading(ui, "device", "simulated");
        return false;
    }
    let backends = c.backends;
    pick_row(ui, "backend", |ui| {
        if choice(
            ui,
            (id, "backend"),
            &mut c.backend,
            backends.iter().enumerate().map(|(i, b)| (i, b.label.to_string())),
        ) {
            c.rescan();
            changed = true;
        }
    });
    if c.has_ports() {
        let labels: Vec<(usize, String)> =
            c.candidates.iter().enumerate().map(|(i, x)| (i, x.label.clone())).collect();
        pick_row(ui, "port", |ui| {
            if labels.is_empty() {
                ui.label(egui_bench::prelude::legend("nothing found"));
            } else {
                choice(ui, (id, "port"), &mut c.target, labels);
            }
        });
    }
    changed
}

pub fn chassis(ui: &Ui) {
    let r = ui.max_rect();
    ui.painter().rect_filled(r, 0.0, egui_bench::prelude::CHASSIS);
    ui.painter().line_segment(
        [r.left_top(), r.right_top()],
        egui::Stroke::new(1.0, egui_bench::prelude::ETCH),
    );
}
