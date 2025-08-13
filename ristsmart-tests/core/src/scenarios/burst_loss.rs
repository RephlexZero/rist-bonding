use anyhow::Result;
use super::Scenario;
use std::time::Duration;

pub struct BurstLoss;

impl BurstLoss {
    pub fn new() -> Self {
        Self
    }
}

impl Scenario for BurstLoss {
    fn name(&self) -> &str { 
        "burst_loss" 
    }

    fn on_tick(&mut self, _elapsed: Duration) -> Result<()> {
        // TODO: Configure burst loss when emulator integration is ready
        Ok(())
    }
}
