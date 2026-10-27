use crate::demo::{DemoLoad, DemoSupply, DemoVna};
use anyhow::{Context, Result};
use antenna_vna::{Port, Vna};
use cycler_core::{Charger, Discharger, open_charger, open_discharger};

pub const DEMO_SPEC: &str = "demo";
pub const DEMO_ON_SPEC: &str = "demo-on";

pub fn open_supply(spec: &str) -> Result<Box<dyn Charger>> {
    match spec {
        DEMO_SPEC => return Ok(Box::new(DemoSupply::new())),
        DEMO_ON_SPEC => return Ok(Box::new(DemoSupply::left_on())),
        _ => {}
    }
    open_charger(spec)
}

pub fn open_load(spec: &str) -> Result<Box<dyn Discharger>> {
    match spec {
        DEMO_SPEC => return Ok(Box::new(DemoLoad::new())),
        DEMO_ON_SPEC => return Ok(Box::new(DemoLoad::left_on())),
        _ => {}
    }
    open_discharger(spec)
}

pub fn open_vna(target: &VnaTarget) -> Result<Box<dyn Vna>> {
    match target {
        VnaTarget::Demo => Ok(Box::new(DemoVna::new())),
        VnaTarget::Port(p) => antenna_vna::open(p).map_err(|e| anyhow::anyhow!("{e}")),
    }
}

#[derive(Clone, Debug)]
pub enum VnaTarget {
    Demo,
    Port(Port),
}

impl VnaTarget {
    pub fn label(&self) -> String {
        match self {
            VnaTarget::Demo => "demo NanoVNA".into(),
            VnaTarget::Port(p) => format!("{} ({})", p.product, p.path),
        }
    }
}

pub fn find_vna(path: Option<&str>) -> Result<VnaTarget> {
    let ports = antenna_vna::detect();
    let port = match path {
        Some(p) => ports.into_iter().find(|x| x.path == p),
        None => ports.into_iter().next(),
    };
    port.map(VnaTarget::Port).context("no NanoVNA found")
}

pub fn vna_targets(demo: bool) -> Vec<VnaTarget> {
    if demo {
        return vec![VnaTarget::Demo];
    }
    antenna_vna::detect().into_iter().map(VnaTarget::Port).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_spec_opens_simulated_devices() {
        assert!(open_supply(DEMO_SPEC).unwrap().name().contains("Demo"));
        assert!(open_load(DEMO_SPEC).unwrap().name().contains("Demo"));
        assert!(open_vna(&VnaTarget::Demo).unwrap().describe().contains("Demo"));
    }

    #[test]
    fn unknown_supply_spec_is_an_error() {
        assert!(open_supply("nope:").is_err());
    }
}
