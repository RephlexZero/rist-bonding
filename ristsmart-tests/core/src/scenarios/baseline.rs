use anyhow::Result;
use super::Scenario;
use std::time::Duration;

pub struct Baseline {
    _nlinks: usize,
}

impl Baseline {
    pub fn new(_nlinks: usize) -> Self {
        Self { _nlinks }
    }
}

impl Scenario for Baseline {
    fn name(&self) -> &str { 
        "baseline" 
    }

    fn on_tick(&mut self, _elapsed: Duration) -> Result<()> {
        // Keep defaults: equal links ~10Mbps, 20ms, low loss
        Ok(())
    }
}
