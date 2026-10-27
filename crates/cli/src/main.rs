use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::time::Duration;
use testbench_core::cycler_core::device::{CHARGER_BACKENDS, LOAD_BACKENDS};
use testbench_core::export::{Z0, sweep_csv, touchstone_s1p};
use testbench_core::limits::{clamp_load, clamp_setpoint};
use testbench_core::open::{DEMO_SPEC, VnaTarget, find_vna, open_load, open_supply, open_vna};
use testbench_core::parse::{parse_freq, parse_mode};
use testbench_core::sweep::SweepParams;

#[derive(Parser)]
#[command(name = "testbench", version, about = "Drive a bench supply, a load and a NanoVNA")]
struct Cli {
    #[arg(long, global = true)]
    demo: bool,
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    Devices,
    Psu {
        #[command(subcommand)]
        action: Psu,
    },
    Load {
        #[command(subcommand)]
        action: LoadCmd,
    },
    Vna {
        #[command(subcommand)]
        action: VnaCmd,
    },
}

#[derive(Subcommand)]
enum Psu {
    Set {
        volts: f64,
        amps: f64,
        #[arg(long)]
        on: bool,
        #[arg(long, default_value = "owon")]
        backend: String,
        #[arg(long)]
        port: Option<String>,
        #[arg(long, default_value_t = 2.0)]
        hold: f64,
    },
    Off {
        #[arg(long, default_value = "owon")]
        backend: String,
        #[arg(long)]
        port: Option<String>,
    },
}

#[derive(Subcommand)]
enum LoadCmd {
    Set {
        mode: String,
        value: f64,
        #[arg(long)]
        on: bool,
        #[arg(long, default_value = "dl24")]
        backend: String,
        #[arg(long)]
        port: Option<String>,
        #[arg(long, default_value_t = 2.0)]
        hold: f64,
    },
    Off {
        #[arg(long, default_value = "dl24")]
        backend: String,
        #[arg(long)]
        port: Option<String>,
    },
}

#[derive(Subcommand)]
enum VnaCmd {
    Sweep {
        start: String,
        stop: String,
        points: usize,
        #[arg(long)]
        csv: Option<PathBuf>,
        #[arg(long)]
        s1p: Option<PathBuf>,
        #[arg(long)]
        port: Option<String>,
    },
}

fn spec(demo: bool, backend: &str, port: &Option<String>) -> String {
    if demo {
        return DEMO_SPEC.into();
    }
    match port {
        Some(p) => format!("{backend}:{p}"),
        None => backend.to_string(),
    }
}

fn devices(demo: bool) -> Result<()> {
    if demo {
        println!("supply  demo");
        println!("load    demo");
        println!("vna     demo");
        return Ok(());
    }
    for (role, backends) in [("supply", CHARGER_BACKENDS), ("load", LOAD_BACKENDS)] {
        for b in backends {
            for c in b.candidates() {
                println!("{role:<7} {}  {}  {}", b.kind, c.target, c.label);
            }
        }
    }
    for p in testbench_core::antenna_vna::detect() {
        println!("vna     {}  {}", p.path, p.product);
    }
    Ok(())
}

fn psu_set(spec: &str, volts: f64, amps: f64, on: bool, hold: f64) -> Result<()> {
    let mut dev = open_supply(spec)?;
    let (v, a) = clamp_setpoint(volts, amps, &dev.limits(), None);
    if (v, a) != (volts, amps) {
        eprintln!("clamped to {v:.3} V {a:.3} A by the device limits");
    }
    dev.set(v, a)?;
    println!("{}: set {v:.3} V {a:.3} A", dev.name());
    if on {
        dev.start()?;
        std::thread::sleep(Duration::from_secs_f64(hold.max(0.0)));
        let m = dev.measure()?;
        println!("measured {:.3} V {:.3} A {:.3} W", m.volts, m.amps, m.volts * m.amps);
        println!("output left on; run `testbench psu off` to stop it");
    }
    Ok(())
}

fn psu_off(spec: &str) -> Result<()> {
    let mut dev = open_supply(spec)?;
    dev.stop()?;
    println!("{}: output off", dev.name());
    Ok(())
}

fn load_set(spec: &str, mode: &str, value: f64, on: bool, hold: f64) -> Result<()> {
    let mode = parse_mode(mode)?;
    let mut dev = open_load(spec)?;
    if !dev.modes().contains(&mode) {
        bail!("{} does not do {}", dev.name(), mode.label());
    }
    let value = clamp_load(mode, value, &dev.limits());
    dev.set_mode(mode, value)?;
    println!("{}: {} {value} {}", dev.name(), mode.label(), mode.unit());
    if on {
        dev.start()?;
        std::thread::sleep(Duration::from_secs_f64(hold.max(0.0)));
        let s = dev.state()?;
        println!(
            "measured {:.3} V {:.3} A {:.3} W {:.4} Ah",
            s.volts, s.amps, s.watts, s.amp_hours
        );
        println!("load left on; run `testbench load off` to stop it");
    }
    Ok(())
}

fn load_off(spec: &str) -> Result<()> {
    let mut dev = open_load(spec)?;
    dev.stop()?;
    println!("{}: off", dev.name());
    Ok(())
}

fn vna_sweep(
    demo: bool,
    start: &str,
    stop: &str,
    points: usize,
    csv: Option<PathBuf>,
    s1p: Option<PathBuf>,
    port: Option<String>,
) -> Result<()> {
    let target = if demo { VnaTarget::Demo } else { find_vna(port.as_deref())? };
    let mut dev = open_vna(&target)?;
    let wanted = SweepParams {
        start_hz: parse_freq(start)?,
        stop_hz: parse_freq(stop)?,
        points,
    };
    let p = wanted.fit(dev.range_hz());
    if p != wanted {
        eprintln!(
            "adjusted to {} to {} Hz, {} points",
            p.start_hz, p.stop_hz, p.points
        );
    }
    let sweep = dev.sweep(p.start_hz, p.stop_hz, p.points, false).map_err(|e| anyhow::anyhow!("{e}"))?;
    let best = sweep
        .iter()
        .min_by(|a, b| a.swr().total_cmp(&b.swr()))
        .context("the sweep returned no points")?;
    println!(
        "{}: {} points, best SWR {:.2} at {:.4} MHz",
        dev.describe(),
        sweep.len(),
        best.swr(),
        best.freq / 1e6
    );
    if let Some(path) = csv {
        std::fs::write(&path, sweep_csv(&sweep, Z0))?;
        println!("wrote {}", path.display());
    }
    if let Some(path) = s1p {
        std::fs::write(&path, touchstone_s1p(&sweep, Z0))?;
        println!("wrote {}", path.display());
    }
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Cmd::Devices => devices(cli.demo),
        Cmd::Psu { action: Psu::Set { volts, amps, on, backend, port, hold } } => {
            psu_set(&spec(cli.demo, &backend, &port), volts, amps, on, hold)
        }
        Cmd::Psu { action: Psu::Off { backend, port } } => psu_off(&spec(cli.demo, &backend, &port)),
        Cmd::Load { action: LoadCmd::Set { mode, value, on, backend, port, hold } } => {
            load_set(&spec(cli.demo, &backend, &port), &mode, value, on, hold)
        }
        Cmd::Load { action: LoadCmd::Off { backend, port } } => load_off(&spec(cli.demo, &backend, &port)),
        Cmd::Vna { action: VnaCmd::Sweep { start, stop, points, csv, s1p, port } } => {
            vna_sweep(cli.demo, &start, &stop, points, csv, s1p, port)
        }
    }
}
