//! Network testbench orchestrator
//!
//! This module provides the main orchestration functionality for creating
//! and managing network namespaces, veth pairs, and impairment schedules.
//! It provides a drop-in replacement for the current NetworkOrchestrator API.

use crate::netns::Manager as NetNsManager;
use crate::veth::PairManager as VethManager;
use crate::addr::{Configurer as AddrConfigurer, AddressConfig};
use crate::qdisc::QdiscManager;
use crate::runtime::Scheduler;
use crate::TestbenchError;
use scenarios::{TestScenario, LinkSpec};
use anyhow::Result;
use ipnetwork::IpNetwork;
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

/// Main network orchestrator providing drop-in compatibility with netlink-sim
pub struct NetworkOrchestrator {
    netns_manager: NetNsManager,
    veth_manager: VethManager,
    addr_configurer: AddrConfigurer,
    qdisc_manager: Arc<QdiscManager>,
    scheduler: Scheduler,
    active_links: Vec<LinkHandle>,
    next_link_id: u64,
    next_port_forward: u16,
    next_port_reverse: u16,
}

impl NetworkOrchestrator {
    /// Create a new network orchestrator
    pub async fn new(seed: u64) -> Result<Self, TestbenchError> {
        info!("Initializing network orchestrator with seed: {}", seed);

        let netns_manager = NetNsManager::new()?;
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
        })
    }

    /// Start a test scenario (drop-in replacement for netlink-sim API)
    pub async fn start_scenario(&mut self, scenario: TestScenario, rx_port: u16) -> Result<LinkHandle, TestbenchError> {
        info!("Starting scenario: {} with rx_port: {}", scenario.name, rx_port);

        let link_id = format!("link_{}", self.next_link_id);
        self.next_link_id += 1;

        // For compatibility, we'll use the first link in the scenario
        let link_spec = scenario.links.first()
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
    pub async fn start_bonding_scenarios(&mut self, scenarios: Vec<TestScenario>, rx_port: u16) -> Result<Vec<LinkHandle>, TestbenchError> {
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

    /// Set up a single link from a LinkSpec
    async fn setup_link(&mut self, link_spec: &LinkSpec, link_id: &str) -> Result<(), TestbenchError> {
        debug!("Setting up link: {} ({} <-> {})", link_spec.name, link_spec.a_ns, link_spec.b_ns);

        // Create namespaces
        let ns_a = format!("{}_{}", link_spec.a_ns, link_id);
        let ns_b = format!("{}_{}", link_spec.b_ns, link_id);
        
        self.netns_manager.create_namespace(&ns_a).await?;
        self.netns_manager.create_namespace(&ns_b).await?;

        // Configure loopback in each namespace
        self.addr_configurer.configure_loopback(&ns_a, &self.netns_manager).await?;
        self.addr_configurer.configure_loopback(&ns_b, &self.netns_manager).await?;

        // Create veth pair
        let veth_a = format!("veth-{}-a", link_id);
        let veth_b = format!("veth-{}-b", link_id);
        
        let _veth_pair = self.veth_manager.create_pair(&veth_a, &veth_b).await?;

        // Move veth ends to namespaces
        self.veth_manager.move_to_namespace(&veth_a, &ns_a, &self.netns_manager).await?;
        self.veth_manager.move_to_namespace(&veth_b, &ns_b, &self.netns_manager).await?;

        // Configure IP addresses (use /30 subnets)
        let (addr_a, addr_b) = AddrConfigurer::generate_p2p_subnet(self.next_link_id as u8)?;

        self.addr_configurer.add_address(AddressConfig {
            interface: veth_a.clone(),
            address: IpNetwork::V4(addr_a),
            namespace: Some(ns_a.clone()),
        }, Some(&self.netns_manager)).await?;

        self.addr_configurer.add_address(AddressConfig {
            interface: veth_b.clone(),
            address: IpNetwork::V4(addr_b),
            namespace: Some(ns_b.clone()),
        }, Some(&self.netns_manager)).await?;

        // Bring interfaces up
        self.veth_manager.set_up(&veth_a, Some(&self.netns_manager)).await?;
        self.veth_manager.set_up(&veth_b, Some(&self.netns_manager)).await?;

        // Set up runtime schedulers for both directions
        // TODO: Get actual interface indices and create proper runtime configs
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
    pub async fn shutdown(self) -> Result<(), TestbenchError> {
        info!("Shutting down orchestrator");
        
        self.scheduler.shutdown().await;
        
        // NetworkOrchestrator cleanup happens in Drop trait
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
/// (drop-in replacement for netlink-sim function)
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
    use scenarios::TestScenario;

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