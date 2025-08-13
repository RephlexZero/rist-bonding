use anyhow::Result;
use crate::emulation::EmuState;

pub struct IdealWeighter {
    efficiency: f64,
    rtt_penalty_ms: f64, // 0 disables RTT penalty
}

impl IdealWeighter {
    pub fn new(efficiency: f64, rtt_penalty_ms: f64) -> Self { 
        Self { efficiency, rtt_penalty_ms } 
    }

    pub fn compute(&self, emu: &EmuState) -> Result<Vec<f64>> {
        // Convert link IP capacity to payload capacity; if RTT penalty > 0, apply 1/(1 + rtt/k)
        // RTT approximated as 2*delay_ms (one-way delay)
        let mut caps: Vec<f64> = emu.capacities_mbps
            .iter()
            .enumerate()
            .map(|(i, mbps)| {
                let payload_bps = mbps * 1_000_000.0 * self.efficiency;
                let rtt_ms = (emu.delay_ms[i] as f64) * 2.0;
                let penalty = if self.rtt_penalty_ms > 0.0 {
                    1.0 / (1.0 + rtt_ms / self.rtt_penalty_ms)
                } else { 
                    1.0 
                };
                payload_bps * penalty
            })
            .collect();
            
        let sum: f64 = caps.iter().sum();
        if sum <= f64::EPSILON {
            Ok(vec![1.0 / (caps.len() as f64); caps.len()])
        } else {
            for c in caps.iter_mut() { 
                *c /= sum; 
            }
            Ok(caps)
        }
    }
}
