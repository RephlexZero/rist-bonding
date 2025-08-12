//! Traffic control (qdisc) management
//!
//! This module provides qdisc management using tc commands in network namespaces.
//! While the original plan called for pure netlink implementation, tc commands
//! are more reliable for complex qdisc configurations.

use crate::errors::{NetemError, Result};
use crate::types::{DelayProfile, GEParams, GeState, RateLimiter};
use crate::util::bps_to_bytes_per_sec;
use tracing::{debug, info};

/// Qdisc manager for a single interface
#[derive(Debug, Clone)]
pub struct QdiscManager {
    if_index: u32,
}

impl QdiscManager {
    pub fn new(if_index: u32) -> Self {
        Self { if_index }
    }

    /// Set up the complete qdisc hierarchy
    pub async fn setup_qdiscs(
        &self,
        rate_limiter: &RateLimiter,
        initial_rate_bps: u64,
        delay_profile: &DelayProfile,
        ge_params: &GEParams,
        ge_state: GeState,
    ) -> Result<()> {
        info!(
            "Setting up qdiscs on interface {}: rate={} bps",
            self.if_index, initial_rate_bps
        );

        // Remove any existing qdiscs
        self.remove_root_qdisc().await?;

        // Add root qdisc (TBF or CAKE)
        match rate_limiter {
            RateLimiter::Tbf => {
                self.add_tbf_qdisc(initial_rate_bps).await?;
            }
            RateLimiter::Cake => {
                self.add_cake_qdisc(initial_rate_bps).await?;
            }
        }

        // Add netem as child qdisc
        self.add_netem_qdisc(delay_profile, ge_params, ge_state)
            .await?;

        debug!("Qdisc setup complete for interface {}", self.if_index);
        Ok(())
    }

    /// Update rate limiting parameters
    pub async fn update_rate(&self, rate_limiter: &RateLimiter, new_rate_bps: u64) -> Result<()> {
        debug!(
            "Updating rate on interface {}: {} bps",
            self.if_index, new_rate_bps
        );

        match rate_limiter {
            RateLimiter::Tbf => {
                self.change_tbf_rate(new_rate_bps).await?;
            }
            RateLimiter::Cake => {
                self.change_cake_rate(new_rate_bps).await?;
            }
        }

        Ok(())
    }

    /// Update netem parameters
    pub async fn update_netem(
        &self,
        delay_profile: &DelayProfile,
        ge_params: &GEParams,
        ge_state: GeState,
    ) -> Result<()> {
        debug!("Updating netem on interface {}", self.if_index);

        // Remove and re-add netem qdisc
        self.remove_netem_qdisc().await?;
        self.add_netem_qdisc(delay_profile, ge_params, ge_state)
            .await?;

        Ok(())
    }

    /// Remove all qdiscs
    pub async fn cleanup(&self) -> Result<()> {
        info!("Cleaning up qdiscs on interface {}", self.if_index);
        self.remove_root_qdisc().await?;
        Ok(())
    }

    /// Add TBF (Token Bucket Filter) qdisc
    async fn add_tbf_qdisc(&self, rate_bps: u64) -> Result<()> {
        let rate_bytes_per_sec = bps_to_bytes_per_sec(rate_bps);
        let burst = (rate_bytes_per_sec / 10).max(1500); // 100ms worth of data, min 1 MTU
        let limit = burst * 3; // Buffer size

        let netns_name = format!("lnk-{}", self.if_index);
        let dev_name = format!("veth{}n", self.if_index);

        run_tc_in_netns(
            &netns_name,
            &[
                "qdisc",
                "add",
                "dev",
                &dev_name,
                "root",
                "handle",
                "1:",
                "tbf",
                "rate",
                &format!("{}bps", rate_bps),
                "burst",
                &format!("{}", burst),
                "limit",
                &format!("{}", limit),
            ],
        )
        .await?;

        debug!("Added TBF qdisc: rate={} bps, burst={}", rate_bps, burst);
        Ok(())
    }

    /// Change TBF rate
    async fn change_tbf_rate(&self, rate_bps: u64) -> Result<()> {
        let rate_bytes_per_sec = bps_to_bytes_per_sec(rate_bps);
        let burst = (rate_bytes_per_sec / 10).max(1500);
        let limit = burst * 3;

        let netns_name = format!("lnk-{}", self.if_index);
        let dev_name = format!("veth{}n", self.if_index);

        run_tc_in_netns(
            &netns_name,
            &[
                "qdisc",
                "change",
                "dev",
                &dev_name,
                "root",
                "handle",
                "1:",
                "tbf",
                "rate",
                &format!("{}bps", rate_bps),
                "burst",
                &format!("{}", burst),
                "limit",
                &format!("{}", limit),
            ],
        )
        .await?;

        debug!("Changed TBF rate to {} bps", rate_bps);
        Ok(())
    }

    /// Add CAKE qdisc
    async fn add_cake_qdisc(&self, rate_bps: u64) -> Result<()> {
        let netns_name = format!("lnk-{}", self.if_index);
        let dev_name = format!("veth{}n", self.if_index);

        run_tc_in_netns(
            &netns_name,
            &[
                "qdisc",
                "add",
                "dev",
                &dev_name,
                "root",
                "handle",
                "1:",
                "cake",
                "bandwidth",
                &format!("{}bit", rate_bps),
            ],
        )
        .await?;

        debug!("Added CAKE qdisc: rate={} bps", rate_bps);
        Ok(())
    }

    /// Change CAKE rate
    async fn change_cake_rate(&self, rate_bps: u64) -> Result<()> {
        let netns_name = format!("lnk-{}", self.if_index);
        let dev_name = format!("veth{}n", self.if_index);

        run_tc_in_netns(
            &netns_name,
            &[
                "qdisc",
                "change",
                "dev",
                &dev_name,
                "root",
                "handle",
                "1:",
                "cake",
                "bandwidth",
                &format!("{}bit", rate_bps),
            ],
        )
        .await?;

        debug!("Changed CAKE rate to {} bps", rate_bps);
        Ok(())
    }

    /// Add netem qdisc as child
    async fn add_netem_qdisc(
        &self,
        delay_profile: &DelayProfile,
        ge_params: &GEParams,
        ge_state: GeState,
    ) -> Result<()> {
        let loss_prob = match ge_state {
            GeState::Good => ge_params.p_good,
            GeState::Bad => ge_params.p_bad,
        };

        // Convert loss probability to percentage for tc
        let loss_pct = loss_prob * 100.0;

        let netns_name = format!("lnk-{}", self.if_index);
        let dev_name = format!("veth{}n", self.if_index);

        let mut args = vec![
            "qdisc", "add", "dev", &dev_name, "parent", "1:1", "handle", "10:", "netem",
        ];

        // Prepare string arguments (need to be alive for the entire call)
        let delay_str = format!("{}ms", delay_profile.delay_ms);
        let jitter_str = format!("{}ms", delay_profile.jitter_ms);
        let loss_str = format!("{:.6}%", loss_pct);
        let reorder_str = format!("{:.2}%", delay_profile.reorder_pct);

        // Add delay
        if delay_profile.delay_ms > 0 {
            args.extend_from_slice(&["delay", &delay_str]);
        }

        // Add jitter
        if delay_profile.jitter_ms > 0 {
            args.push(&jitter_str);
        }

        // Add loss
        if loss_pct > 0.0 {
            args.extend_from_slice(&["loss", &loss_str]);
        }

        // Add reorder
        if delay_profile.reorder_pct > 0.0 {
            args.extend_from_slice(&["reorder", &reorder_str]);
        }

        run_tc_in_netns(&netns_name, &args).await?;

        debug!(
            "Added netem qdisc: delay={}ms, jitter={}ms, loss={:.4}%",
            delay_profile.delay_ms, delay_profile.jitter_ms, loss_pct
        );
        Ok(())
    }

    /// Remove netem qdisc
    async fn remove_netem_qdisc(&self) -> Result<()> {
        let netns_name = format!("lnk-{}", self.if_index);
        let dev_name = format!("veth{}n", self.if_index);

        // Ignore errors when removing (might not exist)
        let _ = run_tc_in_netns(
            &netns_name,
            &[
                "qdisc", "del", "dev", &dev_name, "parent", "1:1", "handle", "10:",
            ],
        )
        .await;

        Ok(())
    }

    /// Remove root qdisc (removes entire hierarchy)
    async fn remove_root_qdisc(&self) -> Result<()> {
        let netns_name = format!("lnk-{}", self.if_index);
        let dev_name = format!("veth{}n", self.if_index);

        // Ignore errors when removing (might not exist)
        let _ = run_tc_in_netns(&netns_name, &["qdisc", "del", "dev", &dev_name, "root"]).await;

        Ok(())
    }
}

/// Helper function to run tc commands in a specific namespace
pub async fn run_tc_in_netns(netns_name: &str, tc_args: &[&str]) -> Result<()> {
    use tokio::process::Command;

    let mut full_args = vec!["netns", "exec", netns_name, "tc"];
    full_args.extend_from_slice(tc_args);

    let output = Command::new("ip")
        .args(&full_args)
        .output()
        .await
        .map_err(|e| NetemError::QdiscApply(format!("Failed to run tc in netns: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(NetemError::QdiscApply(format!(
            "tc command failed: {}",
            stderr
        )));
    }

    Ok(())
}
