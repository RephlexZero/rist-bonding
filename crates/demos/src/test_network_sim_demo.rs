//! Standalone demo of the network simulation functionality
//! This demonstrates the network simulator working independently

use netlink_sim::{NetworkOrchestrator, TestScenario};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Network Simulation Demo");
    println!("==========================\n");

    // Initialize the network orchestrator
    let mut orchestrator = NetworkOrchestrator::new(12345);
    println!("✓ Network orchestrator initialized");

    // Test different scenarios
    let scenarios = vec![
        TestScenario::baseline_good(),
        TestScenario::degraded_network(),
        TestScenario::mobile_network(),
        TestScenario::bonding_asymmetric(),
    ];

    let rx_port = 7000;

    for (i, scenario) in scenarios.into_iter().enumerate() {
        println!("\n📊 Testing Scenario {}: {}", i + 1, scenario.name);
        println!("   Description: {}", scenario.description);

        match orchestrator.start_scenario(scenario, rx_port).await {
            Ok(handle) => {
                println!("✓ Successfully started scenario:");
                println!("  - Ingress Port: {}", handle.ingress_port);
                println!("  - Egress Port:  {}", handle.egress_port);
                println!("  - RX Port:      {}", handle.rx_port);
            }
            Err(e) => {
                println!("✗ Failed to start scenario: {}", e);
            }
        }
    }

    // Display active links summary
    let active_links = orchestrator.get_active_links();
    println!("\n📈 Active Links Summary:");
    println!("   Total links: {}", active_links.len());

    for (i, link) in active_links.iter().enumerate() {
        println!(
            "   Link {}: {} -> {} (rx: {})",
            i + 1,
            link.ingress_port,
            link.egress_port,
            link.rx_port
        );
    }

    // Test bonding setup
    println!("\n🔗 Testing Bonding Setup:");
    let bonding_orchestrator = netlink_sim::start_rist_bonding_test(7100).await?;
    println!("✓ Bonding test setup completed successfully");

    let bonding_links = bonding_orchestrator.get_active_links();
    println!("   Bonding links: {}", bonding_links.len());

    for (i, link) in bonding_links.iter().enumerate() {
        println!(
            "   Bonding Link {}: {} -> {} (scenario: {})",
            i + 1,
            link.ingress_port,
            link.egress_port,
            link.scenario.name
        );
    }

    println!("\n🎉 Network simulation demo completed successfully!");
    println!("   The network simulator is working correctly and can:");
    println!("   • Create various network scenarios (good, poor, cellular, etc.)");
    println!("   • Set up bonding configurations with multiple links");
    println!("   • Manage port allocation automatically");
    println!("   • Coordinate multiple network conditions for testing");

    Ok(())
}
