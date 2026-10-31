# Changelog

All notable changes to Testbench are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.3] - 2026-10-31

### Fixed
- Choice boxes and text fields have accessible names, for screen readers and UI tests.

## [0.1.2] - 2026-10-30

### Changed
- egui and eframe 0.37, which keeps the window running on Wayland when its workspace is hidden.

## [0.1.1] - 2026-10-30

### Fixed
- The window no longer freezes ("not responding") when it sits on another workspace: vsync is off, so a hidden window is never blocked waiting for a frame.

## [0.1.0] - 2026-10-30

### Added
- Supply, load, VNA and log tabs, with a `--demo` mode of simulated devices.
- `testbench` command line tool: `devices`, `psu`, `load` and `vna sweep`.
- A scope tab and `testbench scope devices|idn|capture`.
- Hantek DSO5102P support over its own USB protocol: capture, run, stop and screenshot.
- Rigol, Siglent and Keysight/Owon USBTMC scope drivers (untested on hardware).
- A udev rule for the Hantek in the .deb.
