//! Utility functions and helpers

use crate::errors::{NetemError, Result};
use std::time::{SystemTime, UNIX_EPOCH};

/// Generate a timestamp in milliseconds since epoch
pub fn timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Generate a unique identifier based on timestamp and counter
pub fn generate_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let counter = COUNTER.fetch_add(1, Ordering::SeqCst);
    let timestamp = timestamp_ms();
    format!("{}-{}", timestamp, counter)
}

/// Validate network parameters
pub fn validate_ou_params(ou: &crate::types::OUParams) -> Result<()> {
    if ou.mean_bps == 0 {
        return Err(NetemError::InvalidParameter(
            "mean_bps cannot be zero".to_string(),
        ));
    }
    if ou.tau_ms == 0 {
        return Err(NetemError::InvalidParameter(
            "tau_ms cannot be zero".to_string(),
        ));
    }
    if ou.sigma < 0.0 {
        return Err(NetemError::InvalidParameter(
            "sigma cannot be negative".to_string(),
        ));
    }
    if ou.tick_ms == 0 {
        return Err(NetemError::InvalidParameter(
            "tick_ms cannot be zero".to_string(),
        ));
    }
    Ok(())
}

pub fn validate_ge_params(ge: &crate::types::GEParams) -> Result<()> {
    if !(0.0..=1.0).contains(&ge.p_good) {
        return Err(NetemError::InvalidParameter(
            "p_good must be between 0 and 1".to_string(),
        ));
    }
    if !(0.0..=1.0).contains(&ge.p_bad) {
        return Err(NetemError::InvalidParameter(
            "p_bad must be between 0 and 1".to_string(),
        ));
    }
    if !(0.0..=1.0).contains(&ge.p) {
        return Err(NetemError::InvalidParameter(
            "p must be between 0 and 1".to_string(),
        ));
    }
    if !(0.0..=1.0).contains(&ge.r) {
        return Err(NetemError::InvalidParameter(
            "r must be between 0 and 1".to_string(),
        ));
    }
    Ok(())
}

/// Convert bits per second to bytes per second
pub fn bps_to_bytes_per_sec(bps: u64) -> u64 {
    bps / 8
}

/// Convert milliseconds to nanoseconds for netlink
pub fn ms_to_ns(ms: u32) -> u32 {
    ms * 1_000_000
}

/// Clamp a value between min and max
pub fn clamp<T: PartialOrd>(value: T, min: T, max: T) -> T {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}
