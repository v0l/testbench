mod devices;
mod logtab;
mod load;
mod plot;
mod scope;
mod smith;
mod supply;
mod vna;
mod widgets;

use egui::RichText;
use egui_bench::prelude::*;
use logtab::LogPanel;
use load::LoadPanel;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};
use scope::ScopePanel;
use supply::SupplyPanel;
use testbench_core::worker::{LoadCommand, SupplyCommand};
use vna::VnaPanel;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Supply,
    Load,
    Vna,
    Scope,
    Log,
}

struct App {
    tab: Tab,
    started: Instant,
    supply: SupplyPanel,
    load: LoadPanel,
    vna: VnaPanel,
    scope: ScopePanel,
    log: LogPanel,
}

static SUPPLY_TX: Mutex<Option<Sender<SupplyCommand>>> = Mutex::new(None);
static LOAD_TX: Mutex<Option<Sender<LoadCommand>>> = Mutex::new(None);
static STOP: AtomicBool = AtomicBool::new(false);

fn publish(supply: &SupplyPanel, load: &LoadPanel) {
    if let Ok(mut g) = SUPPLY_TX.lock() {
        *g = supply.session.as_ref().map(|s| s.tx.clone());
    }
    if let Ok(mut g) = LOAD_TX.lock() {
        *g = load.session.as_ref().map(|s| s.tx.clone());
    }
}

fn stop_hardware_on_signal() {
    #[cfg(unix)]
    let signals = [
        signal_hook::consts::SIGINT,
        signal_hook::consts::SIGTERM,
        signal_hook::consts::SIGHUP,
    ];
    #[cfg(not(unix))]
    let signals = [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM];
    for sig in signals {
        let flag = std::sync::Arc::new(AtomicBool::new(false));
        if signal_hook::flag::register(sig, flag.clone()).is_ok() {
            std::thread::spawn(move || {
                while !flag.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(100));
                }
                STOP.store(true, Ordering::Relaxed);
                if let Some(tx) = SUPPLY_TX.lock().ok().and_then(|g| g.clone()) {
                    let _ = tx.send(SupplyCommand::Quit);
                }
                if let Some(tx) = LOAD_TX.lock().ok().and_then(|g| g.clone()) {
                    let _ = tx.send(LoadCommand::Quit);
                }
                std::thread::sleep(Duration::from_millis(1500));
                std::process::exit(0);
            });
        }
    }
}

impl App {
    fn new(demo: bool, tab: Tab) -> Self {
        Self {
            tab,
            started: Instant::now(),
            supply: SupplyPanel::new(demo),
            load: LoadPanel::new(demo),
            vna: VnaPanel::new(demo),
            scope: ScopePanel::new(demo),
            log: LogPanel::new(),
        }
    }

    fn autoconnect(&mut self) {
        self.supply.connect();
        self.load.connect();
        self.vna.connect();
        self.vna.sweep_once();
        self.scope.connect();
    }

    fn now(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    fn drain(&mut self) {
        let t = self.now();
        self.supply.drain(t);
        self.load.drain(t);
        self.vna.drain();
        self.scope.drain();
        publish(&self.supply, &self.load);
        self.log.sample(t, self.supply.reading(), self.load.reading());
    }

    fn shutdown(&mut self) {
        self.log.stop();
        self.supply.disconnect();
        self.load.disconnect();
        self.vna.disconnect();
        self.scope.disconnect();
        publish(&self.supply, &self.load);
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl eframe::App for App {
    fn persist_egui_memory(&self) -> bool {
        false
    }

    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.drain();
        let busy = self.supply.connected() || self.load.connected() || self.vna.connected()
            || self.scope.connected();
        root.ctx().request_repaint_after(Duration::from_millis(if busy { 100 } else { 500 }));

        egui::Panel::top("header")
            .frame(egui::Frame::NONE.fill(CHASSIS).inner_margin(egui::Margin {
                left: 12,
                right: 12,
                top: 8,
                bottom: 6,
            }))
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("TESTBENCH")
                            .font(legend_font(16.0))
                            .extra_letter_spacing(3.0)
                            .color(VALUE),
                    );
                    ui.add_space(16.0);
                    tabs(
                        ui,
                        &mut self.tab,
                        &[
                            (Tab::Supply, "supply"),
                            (Tab::Load, "load"),
                            (Tab::Vna, "vna"),
                            (Tab::Scope, "scope"),
                            (Tab::Log, "log"),
                        ],
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.log.recording() {
                            lamp(ui, "recording", true, true);
                        }
                        if self.supply.last.output == Some(true) {
                            lamp(ui, "supply on", true, false);
                        }
                        if self.load.last.state.is_some_and(|s| s.on) {
                            lamp(ui, "load on", true, false);
                        }
                    });
                });
            });

        match self.tab {
            Tab::Supply => self.supply.ui(root),
            Tab::Load => self.load.ui(root),
            Tab::Vna => self.vna.ui(root),
            Tab::Scope => self.scope.ui(root),
            Tab::Log => {
                let hist = [&self.supply.v_hist, &self.supply.a_hist, &self.load.v_hist, &self.load.a_hist];
                self.log.ui(root, hist);
            }
        }
    }
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let demo = args.iter().any(|a| a == "--demo");
    let connect_scope = args.iter().any(|a| a == "--connect-scope");
    let tab = args
        .iter()
        .position(|a| a == "--tab")
        .and_then(|i| args.get(i + 1))
        .map(|t| match t.as_str() {
            "load" => Tab::Load,
            "vna" => Tab::Vna,
            "scope" => Tab::Scope,
            "log" => Tab::Log,
            _ => Tab::Supply,
        })
        .unwrap_or(Tab::Supply);
    stop_hardware_on_signal();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 700.0])
            .with_min_inner_size([980.0, 620.0])
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon-256.png"))
                    .unwrap_or_default(),
            ),
        glow_options: eframe::egui_glow::GlowConfiguration {
            vsync: false,
            ..Default::default()
        },
        ..Default::default()
    };
    eframe::run_native(
        "testbench",
        options,
        Box::new(move |cc| {
            egui_bench::install(&cc.egui_ctx);
            let mut app = App::new(demo, tab);
            if demo {
                app.autoconnect();
            } else if connect_scope {
                app.scope.connect();
            }
            Ok(Box::new(app))
        }),
    )
}
