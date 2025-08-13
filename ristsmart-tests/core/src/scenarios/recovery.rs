use anyhow::Result;
use super::Scenario;
use std::time::Duration;

pub struct Recovery {
    phase: RecoveryPhase,
    phase_start: Option<f64>,
}

#[derive(Debug, Clone, Copy)]
enum RecoveryPhase {
    Normal,
    Degrading,
    Degraded,
    Recovering,
    Recovered,
}

impl Recovery {
    pub fn new() -> Self {
        Self { 
            phase: RecoveryPhase::Normal,
            phase_start: None,
        }
    }
}

impl Scenario for Recovery {
    fn name(&self) -> &str { 
        "recovery" 
    }

    fn on_tick(&mut self, elapsed: Duration) -> Result<()> {
        let t = elapsed.as_secs_f64();
        
        match self.phase {
            RecoveryPhase::Normal => {
                if t >= 8.0 {
                    self.phase = RecoveryPhase::Degrading;
                    self.phase_start = Some(t);
                }
            }
            RecoveryPhase::Degrading => {
                let start_t = self.phase_start.unwrap();
                let progress = (t - start_t) / 3.0; // 3 second degradation
                
                if progress >= 1.0 {
                    // TODO: Set fully degraded capacity when emulator integration is ready
                    self.phase = RecoveryPhase::Degraded;
                    self.phase_start = Some(t);
                } else {
                    // TODO: Gradual degradation when emulator integration is ready
                    // let cap = 10.0 - (8.0 * progress); // From 10 to 2 Mbps
                }
            }
            RecoveryPhase::Degraded => {
                let start_t = self.phase_start.unwrap();
                if t - start_t >= 5.0 { // Stay degraded for 5 seconds
                    self.phase = RecoveryPhase::Recovering;
                    self.phase_start = Some(t);
                }
            }
            RecoveryPhase::Recovering => {
                let start_t = self.phase_start.unwrap();
                let progress = (t - start_t) / 4.0; // 4 second recovery
                
                if progress >= 1.0 {
                    // TODO: Set fully recovered capacity when emulator integration is ready
                    self.phase = RecoveryPhase::Recovered;
                } else {
                    // TODO: Gradual recovery when emulator integration is ready
                    // let cap = 2.0 + (8.0 * progress); // From 2 to 10 Mbps
                }
            }
            RecoveryPhase::Recovered => {
                // Stay recovered
            }
        }
        
        Ok(())
    }
}
