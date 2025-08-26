//! Network testbench orchestrator
//!
//! This module provides the main orchestration functionality for creating
//! and managing network namespaces, veth pairs, and impairment schedules.
//! It provides a drop-in replacement for the current NetworkOrchestrator API.

use crate::addr::{AddressConfig, Configurer as AddrConfigurer};
use crate::netns::Manager as NetNsManager;
use crate::cellular::CellularProfile;
use crate::qdisc::QdiscManager;
use crate::runtime::Scheduler;
use crate::veth::PairManager as VethManager;
use crate::TestbenchError;
use anyhow::Result;
use ipnetwork::IpNetwork;
use scenarios::{LinkSpec, TestScenario};
use std::sync::Arc;
use tracing::{debug, info};

/// Handle to a running link for external control and monitoring
#[derive(Clone, Debug)]
pub struct LinkHandle {
    pub ingress_port: u16,
    pub egress_port: u16,
    pub rx_port: u16,
    pub scenario: TestScenario,
    pub link_id: String,
}

/// Main network orchestrator providing network simulation capabilities
pub struct NetworkOrchestrator {
    netns_manager: NetNsManager,
    veth_manager: VethManager,
    addr_configurer: AddrConfigurer,
    #[allow(dead_code)]
    qdisc_manager: Arc<QdiscManager>,
    scheduler: Scheduler,
    active_links: Vec<LinkHandle>,
    next_link_id: u64,
    next_port_forward: u16,
    next_port_reverse: u16,
    /// Track resources per link for robust teardown
    link_resources: Vec<LinkResources>,
}

#[derive(Clone, Debug)]
struct LinkResources {
    ns_a: String,
    ns_b: String,
    veth_a: String,
    veth_b: String,
}

impl NetworkOrchestrator {
    /// Create a new network orchestrator
    pub async fn new(seed: u64) -> Result<Self, TestbenchError> {
        info!("Initializing network orchestrator with seed: {}", seed);

        let mut netns_manager = NetNsManager::new()?;

        // Clean up any stale namespaces from previous runs
        let cleaned = netns_manager
            .force_cleanup_stale_namespaces("tx0_link_")
            .await
            .unwrap_or(0)
            + netns_manager
                .force_cleanup_stale_namespaces("rx0_link_")
                .await
                .unwrap_or(0);

        if cleaned > 0 {
            info!("Cleaned up {} stale namespaces", cleaned);
        }

        let veth_manager = VethManager::new().await?;
        let addr_configurer = AddrConfigurer::new().await?;
        let qdisc_manager = Arc::new(QdiscManager::new());
        let scheduler = Scheduler::new();

        // Derive port ranges from seed to avoid conflicts
        let seed_offset = (seed % 1000) as u16;
        let base_forward = 30000u16.saturating_add(seed_offset.saturating_mul(10));
        let base_reverse = 31000u16.saturating_add(seed_offset.saturating_mul(10));

        Ok(Self {
            netns_manager,
            veth_manager,
            addr_configurer,
            qdisc_manager,
            scheduler,
            active_links: Vec::new(),
            next_link_id: 1,
            next_port_forward: base_forward,
            next_port_reverse: base_reverse,
            link_resources: Vec::new(),
        })
    }

    /// Start a test scenario
    pub async fn start_scenario(
        &mut self,
        scenario: TestScenario,
        rx_port: u16,
    ) -> Result<LinkHandle, TestbenchError> {
        info!(
            "Starting scenario: {} with rx_port: {}",
            scenario.name, rx_port
        );

        let link_id = format!("link_{}", self.next_link_id);
        self.next_link_id += 1;

        // For compatibility, we'll use the first link in the scenario
        let link_spec = scenario
            .links
            .first()
            .ok_or_else(|| TestbenchError::InvalidConfig("Scenario has no links".to_string()))?;

        // Set up the link
        self.setup_link(link_spec, &link_id).await?;

        let ingress_port = self.next_port_forward;
        let egress_port = self.next_port_reverse;
        self.next_port_forward += 2;
        self.next_port_reverse += 2;

        let handle = LinkHandle {
            ingress_port,
            egress_port,
            rx_port,
            scenario,
            link_id,
        };

        self.active_links.push(handle.clone());
        info!("Started scenario: {}", handle.scenario.name);

        Ok(handle)
    }

    /// Start multiple scenarios for bonding tests
    pub async fn start_bonding_scenarios(
        &mut self,
        scenarios: Vec<TestScenario>,
        rx_port: u16,
    ) -> Result<Vec<LinkHandle>, TestbenchError> {
        let mut handles = Vec::new();
        for scenario in scenarios {
            let handle = self.start_scenario(scenario, rx_port).await?;
            handles.push(handle);
        }
        Ok(handles)
    }

    /// Get active links
    pub fn get_active_links(&self) -> &[LinkHandle] {
        &self.active_links
    }

    /// Apply a cellular profile to a given link's pair of namespaces/veths
    pub async fn apply_cellular_profile(
        &self,
        link_id: &str,
        profile: &CellularProfile,
    ) -> Result<(), TestbenchError> {
        // Find resources for the link
        let lr = self
            .link_resources
            .iter()
            .find(|lr| lr.veth_a.contains(link_id) && lr.veth_b.contains(link_id))
            .ok_or_else(|| TestbenchError::InvalidConfig(format!(
                "Unknown link id {}",
                link_id
            )))?;

        // Build netem args and HTB/TBF shaper via tc in the namespaces
        let base_dir = self.netns_manager.base_dir_path().clone();

        self.apply_profile_to_iface(&lr.ns_a, &lr.veth_a, profile, &base_dir)
            .await?;
        self.apply_profile_to_iface(&lr.ns_b, &lr.veth_b, profile, &base_dir)
            .await?;

        Ok(())
    }

    async fn apply_profile_to_iface(
        &self,
        ns: &str,
        iface: &str,
        profile: &CellularProfile,
        base_dir: &std::path::Path,
    ) -> Result<(), TestbenchError> {
        use tokio::process::Command;

        // 1) Root HTB class
        let rate = profile.rate_kbit;
        let mut cmd = Command::new("ip");
        cmd.arg("netns").arg("exec").arg(ns).arg("tc");
        cmd.arg("qdisc").arg("replace").arg("dev").arg(iface);
        cmd.arg("root").arg("handle").arg("1:").arg("htb").arg("default").arg("10");
        cmd.env("IP_NETNS_DIR", base_dir);
        let _ = cmd.output().await.map_err(|e| TestbenchError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        // Class
        let mut cmd = Command::new("ip");
        cmd.arg("netns").arg("exec").arg(ns).arg("tc");
        cmd.arg("class").arg("replace").arg("dev").arg(iface);
        cmd.arg("parent").arg("1:").arg("classid").arg("1:10");
        cmd.arg("htb").arg("rate").arg(format!("{}kbit", rate));
        cmd.arg("ceil").arg(format!("{}kbit", rate));
        cmd.env("IP_NETNS_DIR", base_dir);
        let _ = cmd.output().await.map_err(|e| TestbenchError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        // 2) Attach netem to the class with delay/jitter/loss/reorder/duplicate/corrupt
        let mut cmd = Command::new("ip");
        cmd.arg("netns").arg("exec").arg(ns).arg("tc");
        cmd.arg("qdisc").arg("replace").arg("dev").arg(iface);
        cmd.arg("parent").arg("1:10").arg("handle").arg("10:");
        cmd.arg("netem");
        // delay jitter correlation
        cmd.arg("delay")
            .arg(format!("{}ms", profile.delay_ms))
            .arg(format!("{}ms", profile.jitter_ms))
            .arg(format!("{}%", profile.corr_pct));

        // loss model
        match profile.loss.clone() {
            crate::cellular::LossModel::Random { pct, corr_pct } => {
                cmd.arg("loss")
                    .arg(format!("{}%", pct * 100.0))
                    .arg(format!("{}%", corr_pct));
            }
            crate::cellular::LossModel::Gemodel {
                p_enter_bad,
                r_leave_bad,
                bad_loss,
                good_loss,
            } => {
                cmd.arg("loss")
                    .arg("gemodel")
                    .arg(format!("{}", p_enter_bad))
                    .arg(format!("{}", r_leave_bad))
                    .arg(format!("{}", bad_loss))
                    .arg(format!("{}", good_loss));
            }
        }

        // reorder
        if profile.reorder_pct > 0.0 {
            cmd.arg("reorder")
                .arg(format!("{}%", profile.reorder_pct))
                .arg(format!("{}%", profile.reorder_corr_pct));
        }
        // duplicate
        if profile.duplicate_pct > 0.0 {
            cmd.arg("duplicate").arg(format!("{}%", profile.duplicate_pct));
        }
        // corrupt
        if profile.corrupt_pct > 0.0 {
            cmd.arg("corrupt").arg(format!("{}%", profile.corrupt_pct));
        }

        cmd.env("IP_NETNS_DIR", base_dir);
        let out = cmd
            .output()
            .await
            .map_err(|e| TestbenchError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(TestbenchError::InvalidConfig(format!(
                "tc netem apply failed in ns {} on {}: {}",
                ns, iface, stderr
            )));
        }

        Ok(())
    }

    /// Set up a single link from a LinkSpec
    async fn setup_link(
        &mut self,
        link_spec: &LinkSpec,
        link_id: &str,
    ) -> Result<(), TestbenchError> {
        debug!(
            "Setting up link: {} ({} <-> {})",
            link_spec.name, link_spec.a_ns, link_spec.b_ns
        );

        // Create namespaces
        let ns_a = format!("{}_{}", link_spec.a_ns, link_id);
        let ns_b = format!("{}_{}", link_spec.b_ns, link_id);

        self.netns_manager.create_namespace(&ns_a).await?;
        self.netns_manager.create_namespace(&ns_b).await?;

        // Configure loopback in each namespace
        self.addr_configurer
            .configure_loopback(&ns_a, &self.netns_manager)
            .await?;
        self.addr_configurer
            .configure_loopback(&ns_b, &self.netns_manager)
            .await?;

        // Create veth pair (ensure no leftovers from previous runs)
        let veth_a = format!("veth-{}-a", link_id);
        let veth_b = format!("veth-{}-b", link_id);

        // Best-effort cleanup in default namespace if stale
        let _ = self.veth_manager.delete_if_exists(&veth_a).await;
        let _ = self.veth_manager.delete_if_exists(&veth_b).await;

        let _veth_pair = self.veth_manager.create_pair(&veth_a, &veth_b).await?;

        // Move veth ends to namespaces
        self.veth_manager
            .move_to_namespace(&veth_a, &ns_a, &self.netns_manager)
            .await?;
        self.veth_manager
            .move_to_namespace(&veth_b, &ns_b, &self.netns_manager)
            .await?;

        // Configure IP addresses (use /30 subnets)
        // Use the numeric part of the current link id (prior to increment) for deterministic addressing.
        // At this point, self.next_link_id has already been incremented by start_scenario(),
        // so subtract 1 to get the actual link number used in names (e.g., link_1 -> id 1).
        let link_num = (self.next_link_id - 1) as u8;
        let (addr_a, addr_b) = AddrConfigurer::generate_p2p_subnet(link_num)?;

        self.addr_configurer
            .add_address(
                AddressConfig {
                    interface: veth_a.clone(),
                    address: IpNetwork::V4(addr_a),
                    namespace: Some(ns_a.clone()),
                },
                Some(&self.netns_manager),
            )
            .await?;

        self.addr_configurer
            .add_address(
                AddressConfig {
                    interface: veth_b.clone(),
                    address: IpNetwork::V4(addr_b),
                    namespace: Some(ns_b.clone()),
                },
                Some(&self.netns_manager),
            )
            .await?;

        // Bring interfaces up
        self.veth_manager
            .set_up(&veth_a, Some(&self.netns_manager))
            .await?;
        self.veth_manager
            .set_up(&veth_b, Some(&self.netns_manager))
            .await?;

        // Apply basic netem derived from the scenario specs
        // We target both directions: a_ns side interface veth-<id>-a and b_ns side veth-<id>-b
        // Use ip netns exec tc, leveraging Manager's base_dir via IP_NETNS_DIR
        let base_dir = self.netns_manager.base_dir_path().clone();
        self.qdisc_manager
            .apply_netem_in_namespace(&ns_a, &veth_a, &link_spec.a_to_b.initial_spec(), &base_dir)
            .await
            .map_err(|e| TestbenchError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        self.qdisc_manager
            .apply_netem_in_namespace(&ns_b, &veth_b, &link_spec.b_to_a.initial_spec(), &base_dir)
            .await
            .map_err(|e| TestbenchError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        // Set up runtime schedulers for both directions
        // TODO: Get actual interface indices and create proper runtime configs
        // Track resources for teardown
        self.link_resources.push(LinkResources {
            ns_a: ns_a.clone(),
            ns_b: ns_b.clone(),
            veth_a: veth_a.clone(),
            veth_b: veth_b.clone(),
        });

        debug!("Link setup complete: {}", link_spec.name);

        Ok(())
    }

    /// Start the runtime scheduler
    pub async fn start_scheduler(&self) -> Result<(), TestbenchError> {
        self.scheduler.start().await?;
        info!("Started runtime scheduler");
        Ok(())
    }

    /// Shutdown the orchestrator and clean up resources
    pub async fn shutdown(mut self) -> Result<(), TestbenchError> {
        info!("Shutting down orchestrator");

        self.scheduler.shutdown().await;

        // Best-effort teardown of qdiscs, veths, and namespaces
        let base_dir = self.netns_manager.base_dir_path().clone();
        // Remove qdiscs inside namespaces first to release references
        for lr in &self.link_resources {
            let _ = self
                .qdisc_manager
                .remove_all_in_namespace(&lr.ns_a, &lr.veth_a, &base_dir)
                .await;
            let _ = self
                .qdisc_manager
                .remove_all_in_namespace(&lr.ns_b, &lr.veth_b, &base_dir)
                .await;
        }

        // Try to delete veths from default namespace; if they've been moved, this is a no-op.
        for lr in &self.link_resources {
            let _ = self.veth_manager.delete_if_exists(&lr.veth_a).await;
            let _ = self.veth_manager.delete_if_exists(&lr.veth_b).await;
        }

        // Delete namespaces
        for lr in &self.link_resources {
            let _ = self.netns_manager.delete_namespace(&lr.ns_a).await;
            let _ = self.netns_manager.delete_namespace(&lr.ns_b).await;
        }

        // Final sweep for stale namespaces with our prefixes
        let _ = self
            .netns_manager
            .force_cleanup_stale_namespaces("tx0_link_")
            .await;
        let _ = self
            .netns_manager
            .force_cleanup_stale_namespaces("rx0_link_")
            .await;

        Ok(())
    }
}

impl Drop for NetworkOrchestrator {
    fn drop(&mut self) {
        debug!("Cleaning up orchestrator resources");
        // Cleanup happens automatically via Drop traits of manager components
    }
}

/// Convenience function to start a RIST bonding test setup
///
pub async fn start_rist_bonding_test(rx_port: u16) -> Result<NetworkOrchestrator> {
    let mut orchestrator = NetworkOrchestrator::new(42).await?;

    // Create bonding scenario using the scenarios crate
    let scenario = TestScenario::bonding_asymmetric();
    let _handle = orchestrator.start_scenario(scenario, rx_port).await?;

    // Start the scheduler
    orchestrator.start_scheduler().await?;

    info!("RIST bonding test setup complete");
    Ok(orchestrator)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_orchestrator_creation() -> Result<(), TestbenchError> {
        let orchestrator = NetworkOrchestrator::new(123).await?;
        assert!(orchestrator.get_active_links().is_empty());
        Ok(())
    }

    #[tokio::test]
    #[cfg(feature = "sudo-tests")]
    async fn test_scenario_start() -> Result<(), TestbenchError> {
        let mut orchestrator = NetworkOrchestrator::new(456).await?;
        let scenario = TestScenario::baseline_good();

        let handle = orchestrator.start_scenario(scenario, 7000).await?;
        assert_eq!(handle.rx_port, 7000);
        assert_eq!(orchestrator.get_active_links().len(), 1);

        Ok(())
    }
}
