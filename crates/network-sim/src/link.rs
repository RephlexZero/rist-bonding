//! Veth link management across namespaces (Linux only)

#![cfg(target_os = "linux")]

use crate::nsapi::Namespace;
use crate::qdisc::QdiscManager;
use crate::runtime;
use crate::types::{NetworkParams, RuntimeError};
use std::io::Result;
use std::process::Stdio;
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct VethPairConfig {
    pub tx_if: String,
    pub rx_if: String,
    pub tx_ip_cidr: String,
    pub rx_ip_cidr: String,
    pub tx_ns: Option<String>,
    pub rx_ns: Option<String>,
    pub params: Option<NetworkParams>, // optional immediate egress shaping on tx_if
}

#[derive(Debug, Clone)]
pub struct VethPair {
    pub tx_if: String,
    pub rx_if: String,
    pub tx_ns: Option<String>,
    pub rx_ns: Option<String>,
}

impl VethPair {
    pub async fn create(qdisc: &QdiscManager, cfg: &VethPairConfig) -> Result<Self> {
        // Ensure namespaces
        if let Some(tx) = &cfg.tx_ns {
            let _ = Namespace::ensure(tx.clone()).await?;
        }
        if let Some(rx) = &cfg.rx_ns {
            let _ = Namespace::ensure(rx.clone()).await?;
        }

        // Clean any leftovers for both ends in root and target namespaces (best-effort)
        let _ = Command::new("ip")
            .args(["link", "del", "dev", &cfg.tx_if])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
        let _ = Command::new("ip")
            .args(["link", "del", "dev", &cfg.rx_if])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
        if let Some(ns) = &cfg.tx_ns {
            let _ = Command::new("ip")
                .args(["netns", "exec", ns, "ip", "link", "del", "dev", &cfg.tx_if])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .await;
        }
        if let Some(ns) = &cfg.rx_ns {
            let _ = Command::new("ip")
                .args(["netns", "exec", ns, "ip", "link", "del", "dev", &cfg.rx_if])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .await;
        }

        // Create veth pair
        exec_ok(
            "ip",
            &[
                "link", "add", &cfg.tx_if, "type", "veth", "peer", "name", &cfg.rx_if,
            ],
        )
        .await?;

        // Move ends into namespaces as needed
        if let Some(ns) = &cfg.tx_ns {
            exec_ok("ip", &["link", "set", &cfg.tx_if, "netns", ns]).await?;
        }
        if let Some(ns) = &cfg.rx_ns {
            exec_ok("ip", &["link", "set", &cfg.rx_if, "netns", ns]).await?;
        }

        // Configure addresses and bring up
        match &cfg.tx_ns {
            Some(ns) => {
                exec_ok(
                    "ip",
                    &[
                        "netns",
                        "exec",
                        ns,
                        "ip",
                        "addr",
                        "add",
                        &cfg.tx_ip_cidr,
                        "dev",
                        &cfg.tx_if,
                    ],
                )
                .await?;
                exec_ok(
                    "ip",
                    &["netns", "exec", ns, "ip", "link", "set", &cfg.tx_if, "up"],
                )
                .await?;
            }
            None => {
                exec_ok("ip", &["addr", "add", &cfg.tx_ip_cidr, "dev", &cfg.tx_if]).await?;
                exec_ok("ip", &["link", "set", &cfg.tx_if, "up"]).await?;
            }
        }

        match &cfg.rx_ns {
            Some(ns) => {
                exec_ok(
                    "ip",
                    &[
                        "netns",
                        "exec",
                        ns,
                        "ip",
                        "addr",
                        "add",
                        &cfg.rx_ip_cidr,
                        "dev",
                        &cfg.rx_if,
                    ],
                )
                .await?;
                exec_ok(
                    "ip",
                    &["netns", "exec", ns, "ip", "link", "set", &cfg.rx_if, "up"],
                )
                .await?;
                exec_ok(
                    "ip",
                    &["netns", "exec", ns, "ip", "link", "set", "lo", "up"],
                )
                .await?;
            }
            None => {
                exec_ok("ip", &["addr", "add", &cfg.rx_ip_cidr, "dev", &cfg.rx_if]).await?;
                exec_ok("ip", &["link", "set", &cfg.rx_if, "up"]).await?;
            }
        }

        // Optional shaping on tx_if in its namespace
        if let Some(params) = &cfg.params {
            let netem = crate::qdisc::NetemConfig {
                delay_us: params.delay_ms * 1000,
                jitter_us: params.jitter_ms * 1000,
                loss_percent: params.loss_pct * 100.0,
                loss_correlation: params.loss_corr_pct * 100.0,
                reorder_percent: params.reorder_pct * 100.0,
                duplicate_percent: params.duplicate_pct * 100.0,
                rate_bps: params.rate_kbps as u64 * 1000,
            };
            if let Some(ns) = &cfg.tx_ns {
                qdisc
                    .configure_interface_in_ns(ns, &cfg.tx_if, netem)
                    .await
                    .map_err(|e| into_io(e.into()))?;
            } else {
                qdisc
                    .configure_interface(&cfg.tx_if, netem)
                    .await
                    .map_err(|e| into_io(crate::types::RuntimeError::from(e)))?;
            }
        }

        Ok(Self {
            tx_if: cfg.tx_if.clone(),
            rx_if: cfg.rx_if.clone(),
            tx_ns: cfg.tx_ns.clone(),
            rx_ns: cfg.rx_ns.clone(),
        })
    }

    pub async fn clear(&self, qdisc: &QdiscManager) -> Result<()> {
        let _ = runtime::remove_network_params(qdisc, &self.tx_if).await;
        Ok(())
    }

    pub async fn delete(self) -> Result<()> {
        // Delete veth (drop both ends)
        let _ = Command::new("ip")
            .args(["link", "del", "dev", &self.tx_if])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
        // Delete namespaces best-effort
        if let Some(ns) = self.tx_ns {
            let _ = Command::new("ip")
                .args(["netns", "del", &ns])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .await;
        }
        if let Some(ns) = self.rx_ns {
            let _ = Command::new("ip")
                .args(["netns", "del", &ns])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .await;
        }
        Ok(())
    }
}

fn into_io(e: RuntimeError) -> std::io::Error {
    std::io::Error::other(e.to_string())
}

async fn exec_ok(cmd: &str, args: &[&str]) -> Result<()> {
    let out = tokio::process::Command::new(cmd)
        .args(args)
        .output()
        .await?;
    if out.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr);
        Err(std::io::Error::other(format!(
            "{} {:?} failed: {}",
            cmd, args, stderr
        )))
    }
}
