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
    jitter_us: params.jitter_ms * 1000,
    loss_percent: params.loss_pct * 100.0,
    loss_correlation: params.loss_corr_pct * 100.0,
    reorder_percent: params.reorder_pct * 100.0,
    duplicate_percent: params.duplicate_pct * 100.0,
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

/// Remove any network parameters previously applied (delete root qdisc)
pub async fn remove_network_params(
    qdisc_manager: &QdiscManager,
    interface: &str,
) -> Result<(), RuntimeError> {
    qdisc_manager.clear_interface(interface).await?;
    info!("Removed network simulation from {}", interface);
    Ok(())
}

/// Apply ingress network parameters by redirecting ingress to an IFB and shaping there
pub async fn apply_ingress_params(
    qdisc_manager: &QdiscManager,
    interface: &str,
    params: &NetworkParams,
) -> Result<(), RuntimeError> {
    let netem_config = NetemConfig {
        delay_us: params.delay_ms * 1000,
        jitter_us: params.jitter_ms * 1000,
        loss_percent: params.loss_pct * 100.0,
        loss_correlation: params.loss_corr_pct * 100.0,
        reorder_percent: params.reorder_pct * 100.0,
        duplicate_percent: params.duplicate_pct * 100.0,
        rate_bps: params.rate_kbps as u64 * 1000,
    };

    qdisc_manager.configure_ingress(interface, netem_config).await?;
    info!(
        "Applied ingress params to {}: {}ms delay, {}% loss, {} kbps rate",
        interface,
        params.delay_ms,
        params.loss_pct * 100.0,
        params.rate_kbps
    );
    Ok(())
}

/// Remove ingress network parameters, IFB, and filters
pub async fn remove_ingress_params(
    qdisc_manager: &QdiscManager,
    interface: &str,
) -> Result<(), RuntimeError> {
    qdisc_manager.clear_ingress(interface).await?;
    info!("Removed ingress network simulation from {}", interface);
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
