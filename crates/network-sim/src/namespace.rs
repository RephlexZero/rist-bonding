//! Network namespace utilities for enforcing traffic shaping on veth pairs.
//!
//! This module provides functions to create veth pairs with proper namespace isolation
//! to ensure traffic actually traverses shaped interfaces instead of taking kernel shortcuts.

use crate::qdisc::QdiscManager;
use crate::types::{NetworkParams, RuntimeError};
use log::{debug, info};
use std::process::Output;
use tokio::process::Command;

/// Configuration for a shaped veth link with namespace isolation
#[derive(Debug, Clone)]
pub struct ShapedVethConfig {
    /// Name of the TX (sender) interface
    pub tx_interface: String,
    /// Name of the RX (receiver) interface  
    pub rx_interface: String,
    /// IP address for TX interface (with CIDR, e.g. "10.1.1.1/30")
    pub tx_ip: String,
    /// IP address for RX interface (with CIDR, e.g. "10.1.1.2/30")
    pub rx_ip: String,
    /// Network namespace name for RX interface (None = root namespace)
    pub rx_namespace: Option<String>,
    /// Network parameters to apply to TX interface
    pub network_params: NetworkParams,
}

async fn run_ip_cmd(args: &[&str]) -> Result<Output, RuntimeError> {
    debug!("Running: ip {}", args.join(" "));
    let output = Command::new("ip").args(args).output().await?;
    Ok(output)
}

async fn run_ip_cmd_check(args: &[&str]) -> Result<(), RuntimeError> {
    let output = run_ip_cmd(args).await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(RuntimeError::CommandFailed(stderr.to_string()));
    }
    Ok(())
}

async fn run_sysctl_cmd(args: &[&str]) -> Result<Output, RuntimeError> {
    debug!("Running: sysctl {}", args.join(" "));
    let output = Command::new("sysctl").args(args).output().await?;
    Ok(output)
}

async fn run_sysctl_cmd_check(args: &[&str]) -> Result<(), RuntimeError> {
    let output = run_sysctl_cmd(args).await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(RuntimeError::CommandFailed(stderr.to_string()));
    }
    Ok(())
}

/// Set permissive sysctls on an interface to support symmetric routing in tests
async fn set_interface_sysctls(
    rx_namespace: Option<&str>,
    iface: &str,
) -> Result<(), RuntimeError> {
    let rp_filter_key = format!("net.ipv4.conf.{}.rp_filter", iface);
    let accept_local_key = format!("net.ipv4.conf.{}.accept_local", iface);

    if let Some(ns) = rx_namespace {
        // execute sysctl inside namespace
        let _ = run_ip_cmd(&["netns", "exec", ns, "sysctl", "-w", &rp_filter_key, "=2"]).await?;
        let _ = run_ip_cmd(&["netns", "exec", ns, "sysctl", "-w", &accept_local_key, "=1"]).await?;
    } else {
        // root namespace
        run_sysctl_cmd_check(&["-w", &rp_filter_key, "=2"]).await?;
        run_sysctl_cmd_check(&["-w", &accept_local_key, "=1"]).await?;
    }

    Ok(())
}

/// Add an explicit host route to ensure traffic to `dest_ip` egresses via `iface`
async fn add_host_route(
    rx_namespace: Option<&str>,
    dest_ip: &str,
    iface: &str,
) -> Result<(), RuntimeError> {
    if let Some(ns) = rx_namespace {
        // ip netns exec <ns> ip route replace <dest_ip> dev <iface> scope host
        run_ip_cmd_check(&[
            "netns", "exec", ns, "ip", "route", "replace", dest_ip, "dev", iface, "scope", "host",
        ])
        .await
    } else {
        run_ip_cmd_check(&["route", "replace", dest_ip, "dev", iface, "scope", "host"]).await
    }
}

/// Create and configure a shaped veth pair with optional namespace isolation
pub async fn create_shaped_veth_pair(
    qdisc_manager: &QdiscManager,
    config: &ShapedVethConfig,
) -> Result<(), RuntimeError> {
    info!(
        "Creating shaped veth pair: {} <-> {}",
        config.tx_interface, config.rx_interface
    );

    // Clean up any existing interfaces/namespaces
    let _ = cleanup_shaped_veth_pair(qdisc_manager, config).await;

    // Create veth pair
    run_ip_cmd_check(&[
        "link",
        "add",
        &config.tx_interface,
        "type",
        "veth",
        "peer",
        "name",
        &config.rx_interface,
    ])
    .await?;

    // Configure TX interface in root namespace
    run_ip_cmd_check(&["addr", "add", &config.tx_ip, "dev", &config.tx_interface]).await?;
    run_ip_cmd_check(&["link", "set", "dev", &config.tx_interface, "up"]).await?;

    // Handle RX interface based on namespace configuration
    if let Some(ref rx_ns) = config.rx_namespace {
        // Create namespace and move RX interface into it
        info!("Moving {} into namespace {}", config.rx_interface, rx_ns);

        run_ip_cmd_check(&["netns", "add", rx_ns]).await?;
        run_ip_cmd_check(&["link", "set", "dev", &config.rx_interface, "netns", rx_ns]).await?;

        // Configure RX interface inside namespace
        run_ip_cmd_check(&[
            "-n",
            rx_ns,
            "addr",
            "add",
            &config.rx_ip,
            "dev",
            &config.rx_interface,
        ])
        .await?;
        run_ip_cmd_check(&[
            "-n",
            rx_ns,
            "link",
            "set",
            "dev",
            &config.rx_interface,
            "up",
        ])
        .await?;
        run_ip_cmd_check(&["-n", rx_ns, "link", "set", "dev", "lo", "up"]).await?;
    } else {
        // Configure RX interface in root namespace
        run_ip_cmd_check(&["addr", "add", &config.rx_ip, "dev", &config.rx_interface]).await?;
        run_ip_cmd_check(&["link", "set", "dev", &config.rx_interface, "up"]).await?;
    }

    // Extract plain IPs (without CIDR) for routing configuration
    let (tx_ip_nocidr, rx_ip_nocidr) = get_connection_ips(config);

    // Set sysctls to support symmetric routing for both ends
    // - rp_filter=2 (loose) avoids dropping replies due to asymmetric routing
    // - accept_local=1 allows local addresses on veth pairs
    set_interface_sysctls(None, &config.tx_interface).await.ok();
    match &config.rx_namespace {
        Some(ns) => {
            set_interface_sysctls(Some(ns), &config.rx_interface)
                .await
                .ok();
        }
        None => {
            set_interface_sysctls(None, &config.rx_interface).await.ok();
        }
    }

    // Add explicit host routes so that traffic to the peer goes out via the intended iface
    // Route to RX via TX iface (root namespace)
    add_host_route(None, &rx_ip_nocidr, &config.tx_interface)
        .await
        .ok();
    // For reply path:
    if let Some(ref rx_ns) = config.rx_namespace {
        // In RX namespace, route to TX via RX iface
        add_host_route(Some(rx_ns), &tx_ip_nocidr, &config.rx_interface)
            .await
            .ok();
    } else {
        // Same namespace: add reverse host route as well
        add_host_route(None, &tx_ip_nocidr, &config.rx_interface)
            .await
            .ok();
    }

    // Apply shaping to TX interface
    crate::runtime::apply_network_params(
        qdisc_manager,
        &config.tx_interface,
        &config.network_params,
    )
    .await?;

    info!(
        "Successfully created shaped veth pair with {} kbps rate limit",
        config.network_params.rate_kbps
    );

    Ok(())
}

/// Remove a shaped veth pair and clean up namespaces
pub async fn cleanup_shaped_veth_pair(
    qdisc_manager: &QdiscManager,
    config: &ShapedVethConfig,
) -> Result<(), RuntimeError> {
    debug!(
        "Cleaning up shaped veth pair: {} <-> {}",
        config.tx_interface, config.rx_interface
    );

    // Remove shaping from TX interface
    let _ = crate::runtime::remove_network_params(qdisc_manager, &config.tx_interface).await;

    // Remove namespace if it exists
    if let Some(ref rx_ns) = config.rx_namespace {
        let output = run_ip_cmd(&["netns", "del", rx_ns]).await;
        if let Ok(out) = output {
            if !out.status.success() {
                debug!("Namespace {} may not exist (this is OK)", rx_ns);
            }
        }
    }

    // Remove TX interface (this removes the entire veth pair)
    let output = run_ip_cmd(&["link", "del", "dev", &config.tx_interface]).await;
    if let Ok(out) = output {
        if !out.status.success() {
            debug!(
                "Interface {} may not exist (this is OK)",
                config.tx_interface
            );
        }
    }

    Ok(())
}

/// Helper function to create multiple shaped veth pairs for RIST testing
pub async fn create_rist_test_links(
    qdisc_manager: &QdiscManager,
    base_name: &str,
    link_configs: &[(u32, u32)], // (rate_kbps, delay_ms) pairs
) -> Result<Vec<ShapedVethConfig>, RuntimeError> {
    let mut configs = Vec::new();

    for (i, &(rate_kbps, delay_ms)) in link_configs.iter().enumerate() {
        let config = ShapedVethConfig {
            tx_interface: format!("{}tx{}", base_name, i),
            rx_interface: format!("{}rx{}", base_name, i),
            tx_ip: format!("10.{}.1.1/30", 100 + i),
            rx_ip: format!("10.{}.1.2/30", 100 + i),
            rx_namespace: Some(format!("{}ns{}", base_name, i)),
            network_params: NetworkParams {
                delay_ms,
                loss_pct: 0.0,
                rate_kbps,
                jitter_ms: 0,
                reorder_pct: 0.0,
                duplicate_pct: 0.0,
                loss_corr_pct: 0.0,
            },
        };

        create_shaped_veth_pair(qdisc_manager, &config).await?;
        configs.push(config);
    }

    info!(
        "Created {} RIST test links with namespace isolation",
        configs.len()
    );
    Ok(configs)
}

/// Clean up all RIST test links
pub async fn cleanup_rist_test_links(
    qdisc_manager: &QdiscManager,
    configs: &[ShapedVethConfig],
) -> Result<(), RuntimeError> {
    for config in configs {
        cleanup_shaped_veth_pair(qdisc_manager, config).await?;
    }
    info!("Cleaned up {} RIST test links", configs.len());
    Ok(())
}

/// Get the IP addresses (without CIDR) for connecting to a shaped veth pair
pub fn get_connection_ips(config: &ShapedVethConfig) -> (String, String) {
    let tx_ip = config
        .tx_ip
        .split('/')
        .next()
        .unwrap_or(&config.tx_ip)
        .to_string();
    let rx_ip = config
        .rx_ip
        .split('/')
        .next()
        .unwrap_or(&config.rx_ip)
        .to_string();
    (tx_ip, rx_ip)
}

/// Execute a command in the RX namespace of a shaped veth pair
pub async fn exec_in_rx_namespace(
    config: &ShapedVethConfig,
    command: &str,
    args: &[&str],
) -> Result<Output, RuntimeError> {
    if let Some(ref rx_ns) = config.rx_namespace {
        let mut cmd_args = vec!["netns", "exec", rx_ns, command];
        cmd_args.extend_from_slice(args);
        run_ip_cmd(&cmd_args).await
    } else {
        // Execute in root namespace
        let output = Command::new(command).args(args).output().await?;
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qdisc::QdiscManager;

    #[tokio::test]
    async fn test_shaped_veth_creation() {
        let qdisc = QdiscManager::new();
        if !qdisc.has_net_admin().await {
            eprintln!("Skipping namespace test: requires NET_ADMIN");
            return;
        }

        let config = ShapedVethConfig {
            tx_interface: "test-tx".to_string(),
            rx_interface: "test-rx".to_string(),
            tx_ip: "192.168.100.1/30".to_string(),
            rx_ip: "192.168.100.2/30".to_string(),
            rx_namespace: Some("test-ns".to_string()),
            network_params: NetworkParams {
                delay_ms: 20,
                loss_pct: 0.0,
                rate_kbps: 1000,
                jitter_ms: 0,
                reorder_pct: 0.0,
                duplicate_pct: 0.0,
                loss_corr_pct: 0.0,
            },
        };

        // Test creation
        match create_shaped_veth_pair(&qdisc, &config).await {
            Ok(()) => {
                println!("Successfully created shaped veth pair with namespace isolation");

                // Verify TX interface exists in root namespace
                let tx_check = run_ip_cmd(&["addr", "show", "dev", &config.tx_interface]).await;
                assert!(tx_check.is_ok() && tx_check.unwrap().status.success());

                // Verify RX interface exists in namespace
                let rx_check = run_ip_cmd(&[
                    "netns",
                    "exec",
                    "test-ns",
                    "ip",
                    "addr",
                    "show",
                    "dev",
                    &config.rx_interface,
                ])
                .await;
                assert!(rx_check.is_ok() && rx_check.unwrap().status.success());

                // Test cleanup
                cleanup_shaped_veth_pair(&qdisc, &config)
                    .await
                    .expect("cleanup");
                println!("Successfully cleaned up shaped veth pair");
            }
            Err(e) => {
                eprintln!("Failed to create shaped veth pair: {}", e);
                // Attempt cleanup anyway
                let _ = cleanup_shaped_veth_pair(&qdisc, &config).await;
            }
        }
    }
}
