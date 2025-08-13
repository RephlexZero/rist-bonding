use anyhow::Result;
use super::Scenario;
use std::time::Duration;

pub struct TimeVarying;

impl TimeVarying {
    pub fn new() -> Self {
        Self
    }
}

impl Scenario for TimeVarying {
    fn name(&self) -> &str { 
        "time_varying" 
    }

    fn on_tick(&mut self, _elapsed: Duration) -> Result<()> {
        // TODO: Implement time-varying bandwidth when emulator integration is ready
        Ok(())
    }
}
