//! Type definitions for network simulation

use thiserror::Error;

#[derive(Error, Debug)]
pub enum RuntimeError {
    #[error("Qdisc error: {0}")]
    Qdisc(#[from] crate::qdisc::QdiscError),

    #[error("Invalid parameters: {0}")]
    InvalidParams(String),
}

/// Network parameters for simulation
#[derive(Debug, Clone, PartialEq)]
pub struct NetworkParams {
    /// Delay in milliseconds
    pub delay_ms: u32,
    /// Packet loss percentage (0.0 to 1.0)
    pub loss_pct: f32,
    /// Rate limit in kilobits per second
    pub rate_kbps: u32,
}

impl NetworkParams {
    /// Good network conditions
    pub fn good() -> Self {
        Self {
            delay_ms: 5,
            loss_pct: 0.001, // 0.1%
            rate_kbps: 10_000, // 10 Mbps
        }
    }

    /// Typical network conditions
    pub fn typical() -> Self {
        Self {
            delay_ms: 20,
            loss_pct: 0.01, // 1%
            rate_kbps: 5_000, // 5 Mbps
        }
    }

    /// Poor network conditions
    pub fn poor() -> Self {
        Self {
            delay_ms: 100,
            loss_pct: 0.05, // 5%
            rate_kbps: 1_000, // 1 Mbps
        }
    }
}
