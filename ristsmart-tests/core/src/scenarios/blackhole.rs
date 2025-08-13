use anyhow::Result;
use super::Scenario;
use std::time::Duration;

pub struct Blackhole {
    applied: bool,
    recovered: bool,
}

impl Blackhole {
    pub fn new() -> Self {
        Self { 
            applied: false, 
            recovered: false 
        }
    }
}

impl Scenario for Blackhole {
    fn name(&self) -> &str { 
        "blackhole" 
    }

    fn on_tick(&mut self, elapsed: Duration) -> Result<()> {
        let t = elapsed.as_secs_f64();
        
        // t=10s: set link1 capacity to 0; t=13s: restore to 10Mbps
        if t >= 10.0 && !self.applied {
            // TODO: Set link1 capacity to 0 when emulator integration is ready
            self.applied = true;
        }
        
        if t >= 13.0 && !self.recovered {
            // TODO: Restore link1 capacity to 10Mbps when emulator integration is ready
            self.recovered = true;
        }
        
        Ok(())
    }
}
