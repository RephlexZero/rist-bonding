//! Qdisc management for traffic control

use log::{debug, info, warn};
use std::fmt;
use std::process::Output;
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

    #[error("Command failed: {0}")]
    CommandFailed(String),

    #[error("Invalid arguments: {0}")]
    InvalidArgs(String),
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

    async fn run_tc(&self, args: &[&str]) -> Result<Output, QdiscError> {
        debug!("tc {:?}", args);
        let out = Command::new("tc").args(args).output().await?;
        Ok(out)
    }

    async fn run_ip(&self, args: &[&str]) -> Result<Output, QdiscError> {
        debug!("ip {:?}", args);
        let out = Command::new("ip").args(args).output().await?;
        Ok(out)
    }

    async fn interface_exists(&self, interface: &str) -> Result<bool, QdiscError> {
        let out = self
            .run_ip(&["-o", "link", "show", "dev", interface])
            .await?;
        if out.status.success() {
            return Ok(true);
        }
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        if stderr.contains("Cannot find device") || stderr.contains("does not exist") {
            return Ok(false);
        }
        // If ip failed for other reasons, surface it
        Err(QdiscError::CommandFailed(stderr))
    }

    /// Configure network interface with traffic control parameters
    pub async fn configure_interface(
        &self,
        interface: &str,
        config: NetemConfig,
    ) -> Result<(), QdiscError> {
        info!("Configuring interface {} with {}", interface, config);

        // Validate interface presence early
        if !self.interface_exists(interface).await? {
            return Err(QdiscError::InterfaceNotFound(interface.to_string()));
        }

        // Best-effort: delete existing root qdisc (ignore failure)
        let del_out = self
            .run_tc(&["qdisc", "del", "dev", interface, "root"])
            .await;
        if let Ok(out) = del_out {
            if out.status.success() {
                debug!("Deleted existing root qdisc on {}", interface);
            } else {
                debug!("Delete existing qdisc returned {}", out.status);
            }
        } else if let Err(e) = del_out {
            warn!("Failed to run tc delete on {}: {}", interface, e);
        }

        // Prefer netem's built-in rate limiting for simplicity and reliability.
        // We'll attach a single netem qdisc at root and pass along rate/delay/loss/etc.
        // no HTB usage

        // Build netem arguments
        let mut netem_args: Vec<String> =
            vec!["qdisc".into(), "add".into(), "dev".into(), interface.into()];

        // Always attach netem at root
        netem_args.extend(["root".into(), "handle".into(), "10:".into()]);

        netem_args.push("netem".into());

        // Rate limiting (if requested)
        if config.rate_bps > 0 {
            let rate_kbit = (config.rate_bps / 1000).max(1); // kbit/s
            netem_args.push("rate".into());
            netem_args.push(format!("{}kbit", rate_kbit));
        }

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
        let out = self
            .run_tc(&netem_args.iter().map(|s| s.as_str()).collect::<Vec<_>>())
            .await?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            if stderr.contains("Operation not permitted") {
                return Err(QdiscError::PermissionDenied);
            }
            if stderr.contains("Cannot find device") {
                return Err(QdiscError::InterfaceNotFound(interface.to_string()));
            }
            return Err(QdiscError::CommandFailed(stderr.to_string()));
        }

        Ok(())
    }

    /// Configure interface within a netns: `ip netns exec <ns> tc qdisc ...`
    #[cfg(target_os = "linux")]
    pub async fn configure_interface_in_ns(
        &self,
        ns: &str,
        interface: &str,
        config: NetemConfig,
    ) -> Result<(), QdiscError> {
        info!(
            "[ns={}] Configuring interface {} with {}",
            ns, interface, config
        );

        // Delete existing qdisc (best effort)
        let _ = tokio::process::Command::new("ip")
            .args([
                "netns", "exec", ns, "tc", "qdisc", "del", "dev", interface, "root",
            ])
            .output()
            .await;

        // Build netem args similar to configure_interface
        let mut args: Vec<String> = vec![
            "netns".into(),
            "exec".into(),
            ns.into(),
            "tc".into(),
            "qdisc".into(),
            "add".into(),
            "dev".into(),
            interface.into(),
            "root".into(),
            "handle".into(),
            "10:".into(),
            "netem".into(),
        ];

        if config.rate_bps > 0 {
            let rate_kbit = (config.rate_bps / 1000).max(1);
            args.push("rate".into());
            args.push(format!("{}kbit", rate_kbit));
        }
        if config.delay_us > 0 {
            args.push("delay".into());
            args.push(format!("{}us", config.delay_us));
            if config.jitter_us > 0 {
                args.push(format!("{}us", config.jitter_us));
            }
        }
        if config.loss_percent > 0.0 {
            args.push("loss".into());
            args.push(format!("{}%", config.loss_percent));
            if config.loss_correlation > 0.0 {
                args.push(format!("{}%", config.loss_correlation));
            }
        }
        if config.reorder_percent > 0.0 {
            args.push("reorder".into());
            args.push(format!("{}%", config.reorder_percent));
        }
        if config.duplicate_percent > 0.0 {
            args.push("duplicate".into());
            args.push(format!("{}%", config.duplicate_percent));
        }

        let out = tokio::process::Command::new("ip")
            .args(args.iter().map(|s| s.as_str()).collect::<Vec<_>>())
            .output()
            .await?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            if stderr.contains("Operation not permitted") {
                return Err(QdiscError::PermissionDenied);
            }
            if stderr.contains("Cannot find device") {
                return Err(QdiscError::InterfaceNotFound(interface.to_string()));
            }
            return Err(QdiscError::CommandFailed(stderr.to_string()));
        }
        Ok(())
    }

    /// Remove any qdisc configuration from the interface (restore defaults)
    pub async fn clear_interface(&self, interface: &str) -> Result<(), QdiscError> {
        let out = self
            .run_tc(&["qdisc", "del", "dev", interface, "root"])
            .await?;
        if !out.status.success() {
            debug!(
                "No qdisc to delete on {} or insufficient permissions",
                interface
            );
        }
        Ok(())
    }

    /// Remove qdisc configuration for interface within a namespace
    #[cfg(target_os = "linux")]
    pub async fn clear_interface_in_ns(&self, ns: &str, interface: &str) -> Result<(), QdiscError> {
        let out = tokio::process::Command::new("ip")
            .args([
                "netns", "exec", ns, "tc", "qdisc", "del", "dev", interface, "root",
            ])
            .output()
            .await?;
        if !out.status.success() {
            // Best-effort; log/debug
            debug!(
                "[ns={}] clear_interface_in_ns non-success: {}",
                ns, out.status
            );
        }
        Ok(())
    }

    /// Compute the ifb device name to use for ingress shaping on a given interface
    pub fn ingress_ifb_name(&self, interface: &str) -> String {
        let mut base = format!("ifb-{}", interface.replace('/', "-"));
        if base.len() > 15 {
            base.truncate(15);
        }
        base
    }

    async fn ensure_ifb_up(&self, ifb: &str) -> Result<(), QdiscError> {
        // if ifb exists, set up; otherwise create and set up
        match self.interface_exists(ifb).await? {
            true => {
                let _ = self.run_ip(&["link", "set", "dev", ifb, "up"]).await?;
            }
            false => {
                let out = self.run_ip(&["link", "add", ifb, "type", "ifb"]).await?;
                if !out.status.success() {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    if stderr.contains("Operation not permitted") {
                        return Err(QdiscError::PermissionDenied);
                    }
                    return Err(QdiscError::CommandFailed(stderr.to_string()));
                }
                let out = self.run_ip(&["link", "set", "dev", ifb, "up"]).await?;
                if !out.status.success() {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    if stderr.contains("Operation not permitted") {
                        return Err(QdiscError::PermissionDenied);
                    }
                    return Err(QdiscError::CommandFailed(stderr.to_string()));
                }
            }
        }
        Ok(())
    }

    /// Configure ingress shaping by redirecting ingress to an IFB and applying egress shaping on the IFB
    pub async fn configure_ingress(
        &self,
        interface: &str,
        config: NetemConfig,
    ) -> Result<(), QdiscError> {
        info!("Configuring ingress for {} with {}", interface, config);

        if !self.interface_exists(interface).await? {
            return Err(QdiscError::InterfaceNotFound(interface.to_string()));
        }

        // Prepare IFB device
        let ifb = self.ingress_ifb_name(interface);
        self.ensure_ifb_up(&ifb).await?;

        // Add ingress qdisc to base interface (ignore 'File exists')
        let out = self
            .run_tc(&[
                "qdisc", "add", "dev", interface, "handle", "ffff:", "ingress",
            ])
            .await?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            if !(stderr.contains("File exists")
                || stderr.contains("RTNETLINK answers: File exists"))
            {
                if stderr.contains("Operation not permitted") {
                    return Err(QdiscError::PermissionDenied);
                }
                return Err(QdiscError::CommandFailed(stderr.to_string()));
            }
        }

        // Redirect all ingress traffic to IFB (ignore 'File exists')
        let filt_args = [
            "filter", "add", "dev", interface, "parent", "ffff:", "protocol", "all", "u32",
            "match", "u32", "0", "0", "action", "mirred", "egress", "redirect", "dev", &ifb,
        ];
        let out = self.run_tc(&filt_args).await?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            if !(stderr.contains("File exists")
                || stderr.contains("RTNETLINK answers: File exists"))
            {
                if stderr.contains("Operation not permitted") {
                    return Err(QdiscError::PermissionDenied);
                }
                return Err(QdiscError::CommandFailed(stderr.to_string()));
            }
        }

        // Apply shaping on IFB as egress
        self.configure_interface(&ifb, config).await
    }

    /// Clear ingress shaping: remove ingress qdisc/filter and delete IFB device
    pub async fn clear_ingress(&self, interface: &str) -> Result<(), QdiscError> {
        let ifb = self.ingress_ifb_name(interface);

        // Delete ingress qdisc (best-effort)
        let _ = self
            .run_tc(&["qdisc", "del", "dev", interface, "ingress"])
            .await;

        // Clear shaping on IFB
        let _ = self.clear_interface(&ifb).await;

        // Delete IFB device
        let _ = self.run_ip(&["link", "del", "dev", &ifb]).await;

        Ok(())
    }

    /// Return raw `tc qdisc show dev <iface>` output for inspection
    pub async fn describe_interface_qdisc(&self, interface: &str) -> Result<String, QdiscError> {
        let out = self.run_tc(&["qdisc", "show", "dev", interface]).await?;
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        if out.status.success() {
            Ok(stdout)
        } else {
            let stderr = String::from_utf8_lossy(&out.stderr);
            if stderr.contains("Operation not permitted") {
                return Err(QdiscError::PermissionDenied);
            }
            if stderr.contains("Cannot find device") {
                return Err(QdiscError::InterfaceNotFound(interface.to_string()));
            }
            Err(QdiscError::CommandFailed(stderr.to_string()))
        }
    }

    /// Like describe_interface_qdisc, but within a namespace
    #[cfg(target_os = "linux")]
    pub async fn describe_interface_qdisc_in_ns(
        &self,
        ns: &str,
        interface: &str,
    ) -> Result<String, QdiscError> {
        let out = tokio::process::Command::new("ip")
            .args(["netns", "exec", ns, "tc", "qdisc", "show", "dev", interface])
            .output()
            .await
            .map_err(QdiscError::from)?;
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        if out.status.success() {
            Ok(stdout)
        } else {
            let stderr = String::from_utf8_lossy(&out.stderr);
            if stderr.contains("Operation not permitted") {
                return Err(QdiscError::PermissionDenied);
            }
            if stderr.contains("Cannot find device") {
                return Err(QdiscError::InterfaceNotFound(interface.to_string()));
            }
            Err(QdiscError::CommandFailed(stderr.to_string()))
        }
    }

    /// Parse basic counters from `tc -s qdisc show dev <iface>`
    pub async fn get_interface_stats(&self, interface: &str) -> Result<InterfaceStats, QdiscError> {
        let out = self
            .run_tc(&["-s", "qdisc", "show", "dev", interface])
            .await?;
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            if stderr.contains("Operation not permitted") {
                return Err(QdiscError::PermissionDenied);
            }
            if stderr.contains("Cannot find device") {
                return Err(QdiscError::InterfaceNotFound(interface.to_string()));
            }
            return Err(QdiscError::CommandFailed(stderr.to_string()));
        }

        // Very simple parsing: look for "Sent X bytes Y pkt" and "dropped Z"
        let mut sent_bytes = 0u64;
        let mut sent_pkts = 0u64;
        let mut dropped = 0u64;
        for line in stdout.lines() {
            let l = line.trim();
            if l.contains("Sent") && l.contains("bytes") && l.contains("pkt") {
                // Example: "Sent 12345 bytes 67 pkt (dropped 1, overlimits 0 requeues 0)"
                let toks: Vec<&str> = l.split_whitespace().collect();
                for (i, t) in toks.iter().enumerate() {
                    if *t == "Sent" {
                        if let Some(vs) = toks.get(i + 1) {
                            if let Ok(v) = vs.parse::<u64>() {
                                sent_bytes = v;
                            }
                        }
                    }
                    if *t == "pkt" && i > 0 {
                        if let Ok(v) = toks[i - 1].parse::<u64>() {
                            sent_pkts = v;
                        }
                    }
                    if t.contains("dropped") {
                        if let Some(next) = toks.get(i + 1) {
                            let cleaned = next.trim_end_matches([',', ')']);
                            if let Ok(v) = cleaned.parse::<u64>() {
                                dropped = v;
                            }
                        }
                    }
                }
            }
        }

        Ok(InterfaceStats {
            sent_bytes,
            sent_packets: sent_pkts,
            dropped,
        })
    }

    /// Parse basic counters from `tc -s qdisc show dev <iface>` within a namespace
    #[cfg(target_os = "linux")]
    pub async fn get_interface_stats_in_ns(
        &self,
        ns: &str,
        interface: &str,
    ) -> Result<InterfaceStats, QdiscError> {
        let out = tokio::process::Command::new("ip")
            .args([
                "netns", "exec", ns, "tc", "-s", "qdisc", "show", "dev", interface,
            ])
            .output()
            .await
            .map_err(QdiscError::from)?;
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            if stderr.contains("Operation not permitted") {
                return Err(QdiscError::PermissionDenied);
            }
            if stderr.contains("Cannot find device") {
                return Err(QdiscError::InterfaceNotFound(interface.to_string()));
            }
            return Err(QdiscError::CommandFailed(stderr.to_string()));
        }

        let mut sent_bytes = 0u64;
        let mut sent_pkts = 0u64;
        let mut dropped = 0u64;
        for line in stdout.lines() {
            let l = line.trim();
            if l.contains("Sent") && l.contains("bytes") && l.contains("pkt") {
                let toks: Vec<&str> = l.split_whitespace().collect();
                for (i, t) in toks.iter().enumerate() {
                    if *t == "Sent" {
                        if let Some(vs) = toks.get(i + 1) {
                            if let Ok(v) = vs.parse::<u64>() {
                                sent_bytes = v;
                            }
                        }
                    }
                    if *t == "pkt" && i > 0 {
                        if let Ok(v) = toks[i - 1].parse::<u64>() {
                            sent_pkts = v;
                        }
                    }
                    if t.contains("dropped") {
                        if let Some(next) = toks.get(i + 1) {
                            let cleaned = next.trim_end_matches([',', ')']);
                            if let Ok(v) = cleaned.parse::<u64>() {
                                dropped = v;
                            }
                        }
                    }
                }
            }
        }

        Ok(InterfaceStats {
            sent_bytes,
            sent_packets: sent_pkts,
            dropped,
        })
    }

    /// Heuristic check whether tc can be used (indicates NET_ADMIN or equivalent capability)
    pub async fn has_net_admin(&self) -> bool {
        match self.run_tc(&["qdisc", "show"]).await {
            Ok(out) => out.status.success(),
            Err(_) => false,
        }
    }
}

impl Default for QdiscManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Basic interface statistics derived from `tc -s` output
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterfaceStats {
    pub sent_bytes: u64,
    pub sent_packets: u64,
    pub dropped: u64,
}
