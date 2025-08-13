use anyhow::Result;
use super::Scenario;
use std::time::Duration;

pub struct AsymmetricBw {
    initialized: bool,
}

impl AsymmetricBw {
    pub fn new() -> Self {
        Self { initialized: false }
    }
}

impl Scenario for AsymmetricBw {
    fn name(&self) -> &str { 
        "asymmetric_bw" 
    }

    fn on_tick(&mut self, _elapsed: Duration) -> Result<()> {
        if !self.initialized {
            // TODO: Set link0 to 12Mbps, link1 to 3Mbps when emulator integration is ready
            // This would require passing emulator handle to on_tick
            self.initialized = true;
        }
        Ok(())
    }
}
