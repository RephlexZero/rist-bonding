//! Qdisc management for traffic control

use log::{debug, info, warn};
use std::fmt;
use thiserror::Error;
use tokio::process::Command;
use std::process::Output;

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
        let out = self.run_ip(&["-o", "link", "show", "dev", interface]).await?;
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

        // If rate limiting is requested, use HTB root + class, then attach netem under the class.
        // Otherwise, apply netem directly as root.
        let use_htb = config.rate_bps > 0;

        if use_htb {
            let rate_kbit = (config.rate_bps / 1000).max(1); // kbit/s
            let out = self
                .run_tc(&["qdisc", "add", "dev", interface, "root", "handle", "1:", "htb", "default", "10"])
                .await?;
            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr);
                if stderr.contains("Operation not permitted") {
                    return Err(QdiscError::PermissionDenied);
                }
                return Err(QdiscError::CommandFailed(stderr.to_string()));
            }

            let class_args = [
                "class", "add", "dev", interface, "parent", "1:", "classid", "1:10", "htb",
                "rate", &format!("{}kbit", rate_kbit), "ceil", &format!("{}kbit", rate_kbit),
            ];
            let out = self.run_tc(&class_args).await?;
            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr);
                if stderr.contains("Operation not permitted") {
                    return Err(QdiscError::PermissionDenied);
                }
                return Err(QdiscError::CommandFailed(stderr.to_string()));
            }
        }

        // Build netem arguments
        let mut netem_args: Vec<String> = vec!["qdisc".into(), "add".into(), "dev".into(), interface.into()];

        if use_htb {
            netem_args.extend(["parent".into(), "1:10".into(), "handle".into(), "10:".into()]);
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

    /// Remove any qdisc configuration from the interface (restore defaults)
    pub async fn clear_interface(&self, interface: &str) -> Result<(), QdiscError> {
        let out = self
            .run_tc(&["qdisc", "del", "dev", interface, "root"])
            .await?;
        if !out.status.success() {
            debug!("No qdisc to delete on {} or insufficient permissions", interface);
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
            .run_tc(&["qdisc", "add", "dev", interface, "handle", "ffff:", "ingress"])
            .await?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            if !(stderr.contains("File exists") || stderr.contains("RTNETLINK answers: File exists")) {
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
            if !(stderr.contains("File exists") || stderr.contains("RTNETLINK answers: File exists")) {
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
                            if let Ok(v) = vs.parse::<u64>() { sent_bytes = v; }
                        }
                    }
                    if *t == "pkt" && i > 0 {
                        if let Ok(v) = toks[i - 1].parse::<u64>() { sent_pkts = v; }
                    }
                    if t.contains("dropped") {
                        if let Some(next) = toks.get(i + 1) {
                            let cleaned = next.trim_end_matches([',', ')']);
                            if let Ok(v) = cleaned.parse::<u64>() { dropped = v; }
                        }
                    }
                }
            }
        }

        Ok(InterfaceStats { sent_bytes, sent_packets: sent_pkts, dropped })
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
