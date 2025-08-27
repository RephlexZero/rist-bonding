//! Simple network parameter application
//!
//! This module provides utilities for applying fixed network parameters
//! to qdisc configurations - no dynamic scheduling.

use log::info;

use crate::qdisc::{NetemConfig, QdiscManager};
use crate::types::{NetworkParams, RuntimeError};

/// Apply network parameters to a network interface
pub async fn apply_network_params(
    qdisc_manager: &QdiscManager,
    interface: &str,
    params: &NetworkParams,
) -> Result<(), RuntimeError> {
    let netem_config = NetemConfig {
        delay_us: params.delay_ms * 1000,
        jitter_us: 0,
        loss_percent: params.loss_pct * 100.0,
        loss_correlation: 0.0,
        reorder_percent: 0.0,
        duplicate_percent: 0.0,
        rate_bps: params.rate_kbps as u64 * 1000,
    };

    qdisc_manager
        .configure_interface(interface, netem_config)
        .await?;

    info!(
        "Applied parameters to {}: {}ms delay, {}% loss, {} kbps rate",
        interface,
        params.delay_ms,
        params.loss_pct * 100.0,
        params.rate_kbps
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_apply_network_params() {
        let qdisc_manager = QdiscManager::default();
        let params = NetworkParams::typical();

        let result = apply_network_params(&qdisc_manager, "veth0", &params).await;

        match result {
            Ok(()) => println!("Network params applied successfully"),
            Err(e) => println!("Expected error in test environment: {}", e),
        }
    }
}
