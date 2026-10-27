<img src="assets/icon.svg" width="64" alt="">

# testbench

A test and measure desktop app for the bench-suite family. It drives a bench supply, an electronic
load, a NanoVNA and a USB oscilloscope from one window, and has a command line tool for scripting
the same devices.

The device layer is reused, not copied: supplies and loads come from
[cycler-core](https://github.com/v0l/cycler), the NanoVNA from `antenna-vna` in
[antenna-toolbox](https://github.com/v0l/antenna-toolbox), and the oscilloscope drivers live in
`crates/scope`.

## Tabs

- **supply**: pick a backend and port, connect, set voltage and current, switch the output, read
  volts, amps and watts with a CC/CV lamp and a history strip. `arm` writes max volts and amps
  into the supply's own limits so they outlive this program. A supply found on at connect is forced off.
- **load**: pick a backend and port, choose a mode from what the load offers, set the value, switch
  it on, read volts, amps, watts, Ah, Wh and temperature.
- **vna**: detect and connect, set start, stop and points, sweep once or continuously, do a
  short/open/load calibration, read SWR, return loss and a Smith chart, move a marker, export CSV
  and touchstone `.s1p`.
- **scope**: run, stop and single, time and volts per division, trigger, a 10 by 8 graticule screen
  with per-channel traces, an FFT view, measurements and CSV export.
- **log**: record supply and load samples to CSV with a timestamp, and see both on one time axis.

Hardware is stopped when the window closes and on SIGINT, SIGTERM and SIGHUP.

## Run

```
cargo run -p testbench-ui -- --demo
cargo run -p testbench-ui -- --demo --tab scope
```

`--demo` runs simulated devices: a PSU, a load, a NanoVNA with a 14.2 MHz antenna response and a
four channel scope.

## Command line

```
testbench devices
testbench psu set 12 1 --on
testbench psu off
testbench load set cc 0.5 --on
testbench load off
testbench vna sweep 13M 15M 101 --csv out.csv --s1p out.s1p
testbench scope devices
testbench scope idn
testbench scope capture 1 --csv out.csv
```

Add `--demo` to any of them to use the simulated devices.

## Status

Developed without any instrument attached. Verified against the demo devices and unit tests only:

- Supply and load control through cycler-core (OWON SPE, OWON OEL, Atorch DL24), including arm and the left-on safety.
- NanoVNA detection, sweeps and SOL calibration on real hardware.
- USBTMC transport, Rigol, Siglent and Keysight/Owon scope drivers.
- Windows and macOS builds and the release packages.

The scope screen, plots and Smith chart were checked in a headless compositor with demo data.

## Layout

- `crates/core`: logic and workers, no GUI: setpoint clamping, exports, demo models, Smith maths
- `crates/scope`: USBTMC, drivers, measurements, FFT
- `crates/ui`: the egui app (`testbench-ui`)
- `crates/cli`: the `testbench` tool

Licensed under GPL-3.0-or-later.
