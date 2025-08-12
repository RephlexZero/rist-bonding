//! Core types for network emulation

use serde::{Deserialize, Serialize};

/// Ornstein-Uhlenbeck process parameters for throughput variation
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OUParams {
    /// Long-term mean throughput in bits per second
    pub mean_bps: u64,
    /// Mean reversion time constant in milliseconds
    pub tau_ms: u64,
    /// Volatility (fraction of mean per sqrt(second))
    pub sigma: f64,
    /// Controller tick interval in milliseconds
    pub tick_ms: u64,
    /// Optional random seed for deterministic behavior
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
}

impl Default for OUParams {
    fn default() -> Self {
        Self {
            mean_bps: 1_000_000, // 1 Mbps
            tau_ms: 1000,        // 1 second
            sigma: 0.2,          // 20% volatility
            tick_ms: 100,        // 100ms updates
            seed: None,          // Random by default
        }
    }
}

/// Gilbert-Elliott model parameters for burst loss
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GEParams {
    /// Loss probability in GOOD state
    pub p_good: f64,
    /// Loss probability in BAD state  
    pub p_bad: f64,
    /// Transition probability GOOD -> BAD
    pub p: f64,
    /// Transition probability BAD -> GOOD
    pub r: f64,
    /// Optional random seed for deterministic behavior
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
}

impl Default for GEParams {
    fn default() -> Self {
        Self {
            p_good: 0.001, // 0.1% loss in good state
            p_bad: 0.1,    // 10% loss in bad state
            p: 0.01,       // 1% chance to go bad
            r: 0.1,        // 10% chance to recover
            seed: None,    // Random by default
        }
    }
}

/// Gilbert-Elliott state
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GeState {
    Good,
    Bad,
}

/// Delay and jitter profile
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DelayProfile {
    /// Base delay in milliseconds
    pub delay_ms: u32,
    /// Jitter in milliseconds
    pub jitter_ms: u32,
    /// Reorder percentage (0.0-100.0)
    pub reorder_pct: f64,
}

impl Default for DelayProfile {
    fn default() -> Self {
        Self {
            delay_ms: 20,
            jitter_ms: 5,
            reorder_pct: 0.0,
        }
    }
}

impl DelayProfile {
    /// Validate DelayProfile parameters
    pub fn validate(&self) -> Result<(), String> {
        if self.reorder_pct < 0.0 || self.reorder_pct > 100.0 {
            return Err(format!(
                "reorder_pct must be between 0.0 and 100.0, got {}",
                self.reorder_pct
            ));
        }

        if self.jitter_ms > self.delay_ms {
            return Err(format!(
                "jitter_ms ({}) cannot exceed delay_ms ({})",
                self.jitter_ms, self.delay_ms
            ));
        }

        Ok(())
    }
}

/// Rate limiting algorithm
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RateLimiter {
    /// Token Bucket Filter
    Tbf,
    /// Common Applications Kept Enhanced
    Cake,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::Tbf
    }
}

/// Complete link specification
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LinkSpec {
    /// Link identifier
    pub name: String,
    /// Rate limiting algorithm
    pub rate_limiter: RateLimiter,
    /// Ornstein-Uhlenbeck parameters
    pub ou: OUParams,
    /// Gilbert-Elliott parameters
    pub ge: GEParams,
    /// Delay profile
    pub delay: DelayProfile,
    /// Enable ingress shaping via IFB
    pub ifb_ingress: bool,
}

impl LinkSpec {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            rate_limiter: RateLimiter::default(),
            ou: OUParams::default(),
            ge: GEParams::default(),
            delay: DelayProfile::default(),
            ifb_ingress: false,
        }
    }
}

/// Complete scenario specification
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Scenario {
    /// List of links to create
    pub links: Vec<LinkSpec>,
    /// Random seed for reproducibility
    pub seed: Option<u64>,
}
