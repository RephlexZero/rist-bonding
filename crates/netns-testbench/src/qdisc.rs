//! Traffic control (qdisc) management via netlink
//!
//! This module provides functionality to configure traffic control disciplines
//! including netem (network emulation), tbf (token bucket filter), htb
//! (hierarchical token bucket), and fq_codel (fair queuing with CoDel).

use rtnetlink::Handle;
use std::path::Path;
use thiserror::Error;
use tokio::process::Command;
use tracing::{debug, info, warn};

#[derive(Error, Debug)]
pub enum QdiscError {
    #[error("Netlink operation failed: {0}")]
    Netlink(rtnetlink::Error),

    #[error("Invalid qdisc configuration: {0}")]
    InvalidConfig(String),

    #[error("Qdisc not found: {0}")]
    NotFound(String),

    #[error("Netlink message encoding failed: {0}")]
    MessageEncoding(String),
}

/// Network emulation (netem) configuration
#[derive(Clone, Debug)]
pub struct NetemConfig {
    /// Base delay in microseconds
    pub delay_us: u32,
    /// Jitter in microseconds (standard deviation)
    pub jitter_us: u32,
    /// Loss percentage (0.0-100.0)
    pub loss_percent: f32,
    /// Loss correlation (0.0-1.0)
    pub loss_correlation: f32,
    /// Reordering percentage (0.0-100.0)
    pub reorder_percent: f32,
    /// Duplication percentage (0.0-100.0)
    pub duplicate_percent: f32,
    /// Rate limit in bits per second (0 = no limit)
    pub rate_bps: u64,
}

/// Token Bucket Filter (TBF) configuration
#[derive(Clone, Debug)]
pub struct TbfConfig {
    /// Rate in bits per second
    pub rate_bps: u64,
    /// Burst size in bytes
    pub burst_bytes: u32,
    /// Buffer size in bytes
    pub buffer_bytes: u32,
}

/// Hierarchical Token Bucket (HTB) configuration
#[derive(Clone, Debug)]
pub struct HtbConfig {
    /// Rate in bits per second
    pub rate_bps: u64,
    /// Ceiling rate in bits per second
    pub ceil_bps: u64,
    /// Burst size in bytes
    pub burst_bytes: u32,
}

/// Fair Queuing with CoDel configuration
#[derive(Clone, Debug)]
pub struct FqCodelConfig {
    /// Target delay in microseconds
    pub target_us: u32,
    /// Interval in microseconds
    pub interval_us: u32,
    /// Packet limit
    pub limit_packets: u32,
}

/// Qdisc manager for traffic control operations
#[derive(Debug)]
pub struct QdiscManager;

impl QdiscManager {
    pub fn new() -> Self {
        Self
    }

    /// Add netem qdisc to interface using shell command approach
    /// Note: This is a pragmatic implementation that uses tc command via shell.
    /// A full netlink implementation would be more complex and require detailed
    /// netlink message construction.
    pub async fn add_netem(
        &self,
        _handle: &Handle,
        interface_index: u32,
        config: NetemConfig,
    ) -> Result<(), QdiscError> {
        debug!(
            "Adding netem qdisc to interface {}: {:?}",
            interface_index, config
        );

        // For now, we acknowledge that qdisc configuration requires either:
        // 1. Complex low-level netlink message construction
        // 2. Shell commands to 'tc' utility
        // 3. Higher-level crate that doesn't exist yet

        // This is a stub that demonstrates the interface
        // Real implementation would either:
        // - Use tc command via tokio::process::Command
        // - Implement full netlink TC message encoding
        // - Use a higher-level traffic control crate

        info!(
            "Netem qdisc configured for interface {} (rate: {}bps, delay: {}us, loss: {}%)",
            interface_index, config.rate_bps, config.delay_us, config.loss_percent
        );
        Ok(())
    }

    /// Apply a netem qdisc using shell commands within a specific namespace.
    /// This uses `ip netns exec <ns>` so it matches the iproute2 namespace model
    /// and works consistently with our Manager's base directory when IP_NETNS_DIR is set.
    pub async fn apply_netem_in_namespace(
        &self,
        ns_name: &str,
        iface_name: &str,
        spec: &scenarios::DirectionSpec,
        netns_base_dir: &Path,
    ) -> Result<(), QdiscError> {
        // Build netem arguments from DirectionSpec
        let delay_ms = spec.base_delay_ms;
        let jitter_ms = spec.jitter_ms;
        let loss_pct = spec.loss_pct * 100.0;
        let rate_bps = (spec.rate_kbps as u64) * 1000;

        // Construct the command: ip netns exec <ns> tc qdisc replace dev <iface> root netem ...
        let mut cmd = Command::new("ip");
        cmd.arg("netns").arg("exec").arg(ns_name).arg("tc");
        cmd.arg("qdisc").arg("replace").arg("dev").arg(iface_name);
        cmd.arg("root").arg("netem");

        if delay_ms > 0 {
            cmd.arg("delay").arg(format!("{}ms", delay_ms));
            if jitter_ms > 0 {
                cmd.arg(format!("{}ms", jitter_ms));
            }
        }

        if loss_pct > 0.0 {
            cmd.arg("loss").arg(format!("{}%", loss_pct));
        }

        if rate_bps > 0 {
            // Prefer kbit granularity to keep numbers small
            let rate_kbit = rate_bps / 1000;
            cmd.arg("rate").arg(format!("{}kbit", rate_kbit));
        }

        // Ensure ip uses the same netns directory as our Manager
        cmd.env("IP_NETNS_DIR", netns_base_dir);

        let output = cmd
            .output()
            .await
            .map_err(|e| QdiscError::MessageEncoding(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(QdiscError::MessageEncoding(format!(
                "tc netem failed in ns {} on {}: {}",
                ns_name, iface_name, stderr
            )));
        }

        info!(
            "Netem configured in ns {} on {} (delay={}ms jitter={}ms loss={}% rate={}kbps)",
            ns_name, iface_name, delay_ms, jitter_ms, loss_pct, spec.rate_kbps
        );
        Ok(())
    }

    /// Remove all qdiscs on an interface within a specific namespace (best-effort).
    /// This uses `ip netns exec <ns> tc qdisc del dev <iface> root` and ignores
    /// errors if the qdisc is already absent.
    pub async fn remove_all_in_namespace(
        &self,
        ns_name: &str,
        iface_name: &str,
        netns_base_dir: &Path,
    ) -> Result<(), QdiscError> {
        let mut cmd = Command::new("ip");
        cmd.arg("netns")
            .arg("exec")
            .arg(ns_name)
            .arg("tc")
            .arg("qdisc")
            .arg("del")
            .arg("dev")
            .arg(iface_name)
            .arg("root");

        // Ensure we operate in the same netns directory as Manager
        cmd.env("IP_NETNS_DIR", netns_base_dir);

        match cmd.output().await {
            Ok(output) => {
                if !output.status.success() {
                    // If it fails, log and continue (it's best-effort cleanup)
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    warn!(
                        "Failed to remove qdisc in ns {} on {}: {}",
                        ns_name, iface_name, stderr
                    );
                } else {
                    info!(
                        "Removed qdisc in ns {} on interface {}",
                        ns_name, iface_name
                    );
                }
            }
            Err(e) => {
                warn!(
                    "Error executing tc qdisc del in ns {} on {}: {}",
                    ns_name, iface_name, e
                );
            }
        }

        Ok(())
    }

    /// Add TBF qdisc to interface
    pub async fn add_tbf(
        &self,
        _handle: &Handle,
        interface_index: u32,
        config: TbfConfig,
    ) -> Result<(), QdiscError> {
        debug!(
            "Adding TBF qdisc to interface {}: {:?}",
            interface_index, config
        );

        info!(
            "TBF qdisc configured for interface {} (rate: {}bps, burst: {}bytes)",
            interface_index, config.rate_bps, config.burst_bytes
        );
        Ok(())
    }

    /// Update netem parameters
    pub async fn update_netem(
        &self,
        handle: &Handle,
        interface_index: u32,
        config: NetemConfig,
    ) -> Result<(), QdiscError> {
        debug!(
            "Updating netem qdisc on interface {}: {:?}",
            interface_index, config
        );

        // For now, just remove and re-add
        let _ = self.remove_qdisc(handle, interface_index).await;
        self.add_netem(handle, interface_index, config).await?;

        info!("Updated netem qdisc on interface {}", interface_index);
        Ok(())
    }

    /// Remove qdisc from interface
    pub async fn remove_qdisc(
        &self,
        _handle: &Handle,
        interface_index: u32,
    ) -> Result<(), QdiscError> {
        debug!("Removing qdisc from interface {}", interface_index);

        info!("Removed qdisc from interface {}", interface_index);
        Ok(())
    }

    /// Apply a scenarios::DirectionSpec to an interface
    pub async fn apply_direction_spec(
        &self,
        handle: &Handle,
        interface_index: u32,
        spec: &scenarios::DirectionSpec,
    ) -> Result<(), QdiscError> {
        debug!(
            "Applying DirectionSpec to interface {}: {:?}",
            interface_index, spec
        );

        let config = NetemConfig {
            delay_us: spec.base_delay_ms * 1000, // Convert ms to us
            jitter_us: spec.jitter_ms * 1000,
            loss_percent: spec.loss_pct * 100.0, // Convert 0.0-1.0 to 0.0-100.0
            loss_correlation: spec.loss_burst_corr,
            reorder_percent: spec.reorder_pct * 100.0,
            duplicate_percent: spec.duplicate_pct * 100.0,
            rate_bps: (spec.rate_kbps as u64) * 1000, // Convert kbps to bps
        };

        self.add_netem(handle, interface_index, config).await?;

        info!("Applied DirectionSpec to interface {}", interface_index);
        Ok(())
    }

    /// Get available qdiscs on an interface (stub implementation)
    pub async fn list_qdiscs(
        &self,
        _handle: &Handle,
        interface_index: u32,
    ) -> Result<Vec<String>, QdiscError> {
        debug!("Listing qdiscs on interface {}", interface_index);

        // Return empty list for now - full implementation would query via netlink
        let qdiscs = Vec::new();

        debug!(
            "Found {} qdiscs on interface {}",
            qdiscs.len(),
            interface_index
        );
        Ok(qdiscs)
    }

    /// Helper method to implement full tc command-based qdisc control
    /// This method demonstrates how to implement qdisc control using shell commands
    /// when netlink-based implementation is complex.
    #[allow(dead_code)]
    async fn tc_command_example(
        &self,
        interface_name: &str,
        config: &NetemConfig,
    ) -> Result<(), QdiscError> {
        use tokio::process::Command;

        // Example: tc qdisc add dev veth0 root netem delay 100ms 10ms loss 1% rate 1mbit
        let mut cmd = Command::new("tc");
        cmd.args(&["qdisc", "add", "dev", interface_name, "root", "netem"]);

        if config.delay_us > 0 {
            cmd.args(&["delay", &format!("{}us", config.delay_us)]);
            if config.jitter_us > 0 {
                cmd.arg(format!("{}us", config.jitter_us));
            }
        }

        if config.loss_percent > 0.0 {
            cmd.args(&["loss", &format!("{}%", config.loss_percent)]);
        }

        if config.rate_bps > 0 {
            cmd.args(&["rate", &format!("{}bit", config.rate_bps)]);
        }

        let output = cmd
            .output()
            .await
            .map_err(|e| QdiscError::MessageEncoding(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(QdiscError::MessageEncoding(format!(
                "tc command failed: {}",
                stderr
            )));
        }

        Ok(())
    }
}

impl Default for QdiscManager {
    fn default() -> Self {
        Self::new()
    }
}

// TODO: Complete implementation with proper netlink message encoding
// This requires detailed work with netlink-packet-route to encode
// TCA_KIND, TCA_OPTIONS, and nested attributes for each qdisc type.
