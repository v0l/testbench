use testbench_core::cycler_core::discover::{Backend, Candidate, Transport};
use testbench_core::open::{DEMO_SPEC, VnaTarget, vna_targets};

pub struct Choice {
    pub backends: &'static [Backend],
    pub backend: usize,
    pub candidates: Vec<Candidate>,
    pub target: usize,
    pub demo: bool,
}

impl Choice {
    pub fn new(backends: &'static [Backend], demo: bool) -> Self {
        let mut c = Self { backends, backend: 0, candidates: Vec::new(), target: 0, demo };
        c.rescan();
        c
    }

    pub fn backend(&self) -> Option<&Backend> {
        self.backends.get(self.backend)
    }

    pub fn has_ports(&self) -> bool {
        self.backend().is_some_and(|b| b.transport != Transport::Manual)
    }

    pub fn rescan(&mut self) {
        let keep = self.candidates.get(self.target).map(|c| c.target.clone());
        self.candidates = self.backend().map(|b| b.candidates()).unwrap_or_default();
        self.target = keep
            .and_then(|t| self.candidates.iter().position(|c| c.target == t))
            .unwrap_or(0);
    }

    pub fn spec(&self) -> Option<String> {
        if self.demo {
            return Some(DEMO_SPEC.into());
        }
        let b = self.backend()?;
        let target = self.candidates.get(self.target).map(|c| c.target.as_str());
        match (target, b.transport) {
            (None, t) if t != Transport::Manual => None,
            (t, _) => Some(b.spec(t.unwrap_or(""))),
        }
    }
}

pub struct VnaChoice {
    pub targets: Vec<VnaTarget>,
    pub picked: usize,
    demo: bool,
}

impl VnaChoice {
    pub fn new(demo: bool) -> Self {
        Self { targets: vna_targets(demo), picked: 0, demo }
    }

    pub fn rescan(&mut self) {
        self.targets = vna_targets(self.demo);
        self.picked = self.picked.min(self.targets.len().saturating_sub(1));
    }

    pub fn selected(&self) -> Option<VnaTarget> {
        self.targets.get(self.picked).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use testbench_core::cycler_core::device::CHARGER_BACKENDS;

    #[test]
    fn demo_choice_always_has_a_spec() {
        let c = Choice::new(CHARGER_BACKENDS, true);
        assert_eq!(c.spec().as_deref(), Some(DEMO_SPEC));
    }

    #[test]
    fn a_serial_backend_with_nothing_plugged_in_has_no_spec() {
        let mut c = Choice::new(CHARGER_BACKENDS, false);
        c.candidates.clear();
        assert_eq!(c.spec(), None);
    }

    #[test]
    fn demo_vna_choice_is_the_demo_target() {
        let c = VnaChoice::new(true);
        assert!(matches!(c.selected(), Some(VnaTarget::Demo)));
    }
}
