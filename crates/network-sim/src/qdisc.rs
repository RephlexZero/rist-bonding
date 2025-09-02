//! Qdisc management for traffic control

use log::{debug, info, warn};
use std::fmt;
use thiserror::Error;
use tokio::process::Command;

#[derive(Error, Debug)]
pub enum QdiscError {
    #[error("Command error: {0}")]
    Command(#[from] std::io::Error),

    #[error("Interface not found: {0}")]
    InterfaceNotFound(String),

    #[error("Permission denied (requires root privileges)")]
    PermissionDenied,
}

/// Network emulation configuration
#[derive(Debug, Clone)]
pub struct NetemConfig {
    pub delay_us: u32,
    pub jitter_us: u32,
    pub loss_percent: f32,
    pub loss_correlation: f32,
    pub reorder_percent: f32,
    pub duplicate_percent: f32,
    pub rate_bps: u64,
}

impl fmt::Display for NetemConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "NetemConfig {{ delay: {}us, loss: {}%, rate: {} bps }}",
            self.delay_us, self.loss_percent, self.rate_bps
        )
    }
}

/// Manager for qdisc traffic control
pub struct QdiscManager {}

impl QdiscManager {
    pub fn new() -> Self {
        Self {}
    }

    /// Configure network interface with traffic control parameters
    pub async fn configure_interface(
        &self,
        interface: &str,
        config: NetemConfig,
    ) -> Result<(), QdiscError> {
        info!("Configuring interface {} with {}", interface, config);

        // Best-effort: delete existing root qdisc (ignore failure)
        let del_status = Command::new("tc")
            .args(["qdisc", "del", "dev", interface, "root"])
            .status()
            .await;
        match del_status {
            Ok(status) if !status.success() => {
                debug!("No existing qdisc to delete or delete failed with status: {}", status);
            }
            Ok(_) => debug!("Deleted existing root qdisc on {}", interface),
            Err(e) => warn!("Failed to run tc delete on {}: {}", interface, e),
        }

        // If rate limiting is requested, create a TBF as root and attach netem as child.
        // Otherwise, apply netem directly as root.
        let apply_netem_as_child = config.rate_bps > 0;

        if apply_netem_as_child {
            let rate_kbit = (config.rate_bps / 1000).max(1); // kbit/s
            // Sensible defaults for burst/latency
            let burst_bytes = 32 * 1024; // 32KB
            let latency_ms = 50u32; // 50ms

            let tbf_args = [
                "qdisc", "add", "dev", interface, "root", "handle", "1:", "tbf",
                "rate",
                &format!("{}kbit", rate_kbit),
                "burst",
                &format!("{}b", burst_bytes),
                "latency",
                &format!("{}ms", latency_ms),
            ];

            let status = Command::new("tc").args(tbf_args).status().await?;
            if !status.success() {
                return Err(QdiscError::PermissionDenied);
            }
        }

        // Build netem arguments
        let mut netem_args: Vec<String> = vec![
            "qdisc".into(),
            "add".into(),
            "dev".into(),
            interface.into(),
        ];

        if apply_netem_as_child {
            netem_args.extend(["parent".into(), "1:1".into(), "handle".into(), "10:".into()]);
        } else {
            netem_args.extend(["root".into(), "handle".into(), "10:".into()]);
        }

        netem_args.push("netem".into());

        // Delay and optional jitter
        if config.delay_us > 0 {
            netem_args.push("delay".into());
            netem_args.push(format!("{}us", config.delay_us));
            if config.jitter_us > 0 {
                netem_args.push(format!("{}us", config.jitter_us));
            }
        }

        // Packet loss
        if config.loss_percent > 0.0 {
            netem_args.push("loss".into());
            netem_args.push(format!("{}%", config.loss_percent));
            // netem supports optional correlation; keep it simple for now
            if config.loss_correlation > 0.0 {
                netem_args.push(format!("{}%", config.loss_correlation));
            }
        }

        // Reorder
        if config.reorder_percent > 0.0 {
            netem_args.push("reorder".into());
            netem_args.push(format!("{}%", config.reorder_percent));
        }

        // Duplicate
        if config.duplicate_percent > 0.0 {
            netem_args.push("duplicate".into());
            netem_args.push(format!("{}%", config.duplicate_percent));
        }

        debug!("Applying netem with args: {:?}", netem_args);
        let status = Command::new("tc").args(&netem_args).status().await?;
        if !status.success() {
            return Err(QdiscError::PermissionDenied);
        }

        Ok(())
    }

    /// Remove any qdisc configuration from the interface (restore defaults)
    pub async fn clear_interface(&self, interface: &str) -> Result<(), QdiscError> {
        let status = Command::new("tc")
            .args(["qdisc", "del", "dev", interface, "root"])
            .status()
            .await?;
        if !status.success() {
            debug!("No qdisc to delete on {} or insufficient permissions", interface);
        }
        Ok(())
    }
}

impl Default for QdiscManager {
    fn default() -> Self {
        Self::new()
    }
}
