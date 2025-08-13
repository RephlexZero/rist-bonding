use anyhow::Result;
use super::Scenario;
use std::time::Duration;

pub struct HighRtt;

impl HighRtt {
    pub fn new() -> Self {
        Self
    }
}

impl Scenario for HighRtt {
    fn name(&self) -> &str { 
        "high_rtt" 
    }

    fn on_tick(&mut self, _elapsed: Duration) -> Result<()> {
        // TODO: Set high delay when emulator integration is ready
        Ok(())
    }
}
