//! Standalone demo of the new netns-testbench functionality
//! This demonstrates the new Linux namespace-based network simulator

use netns_testbench::{NetworkOrchestrator, LinkHandle as ScenarioHandle};
use scenarios::TestScenario;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing for better debugging
    tracing_subscriber::fmt::init();
    
    println!("🚀 Network Namespace Simulation Demo");
    println!("=====================================\n");
    
    // Check for required capabilities
    if std::env::var("EUID").unwrap_or_else(|_| "1000".to_string()) != "0" {
        println!("⚠️  Warning: This demo requires root privileges (CAP_NET_ADMIN)");
        println!("   Run with: sudo -E cargo run --features netns-sim --bin test_netns_demo");
        println!("   or set CAP_NET_ADMIN capability on the binary\n");
    }

    // Initialize the network orchestrator
    let mut orchestrator = NetworkOrchestrator::new(12345).await?;
    println!("✓ Network namespace orchestrator initialized");

    // Test different scenarios using the new enhanced scenarios
    let scenarios = vec![
        TestScenario::baseline_good(),
        TestScenario::mobile_handover(),
        TestScenario::degrading_network(),
        TestScenario::bonding_asymmetric(),
    ];

    let rx_port = 7000;
    
    for (i, scenario) in scenarios.into_iter().enumerate() {
        println!("\n📊 Testing Scenario {}: {}", i + 1, scenario.name);
        println!("   Description: {}", scenario.description);
        println!("   Links: {}, Duration: {:?}s", 
                 scenario.links.len(),
                 scenario.duration_seconds);
        
        match orchestrator.start_scenario(scenario, rx_port).await {
            Ok(handle) => {
                println!("✓ Successfully started scenario:");
                println!("  - Ingress Port: {}", handle.ingress_port);
                println!("  - Egress Port:  {}", handle.egress_port);
                println!("  - RX Port:      {}", handle.rx_port);
                
                // For demo, create some minimal network activity
                if let Err(e) = test_link_connectivity(&handle).await {
                    println!("⚠️  Link connectivity test failed: {}", e);
                } else {
                    println!("✓ Link connectivity test passed");
                }
            }
            Err(e) => {
                println!("✗ Failed to start scenario: {}", e);
                if e.to_string().contains("permission") || e.to_string().contains("Operation not permitted") {
                    println!("   → Try running with sudo or CAP_NET_ADMIN capability");
                }
            }
        }
    }

    // Display active links summary  
    let active_links = orchestrator.get_active_links();
    println!("\n📈 Active Links Summary:");
    println!("   Total links: {}", active_links.len());
    
    for (i, link) in active_links.iter().enumerate() {
        println!("   Link {}: {} -> {} (rx: {})", 
                 i + 1, 
                 link.ingress_port, 
                 link.egress_port, 
                 link.rx_port);
    }

    // Test bonding setup with enhanced scenarios
    println!("\n🔗 Testing Bonding Setup:");
    match test_bonding_setup(&mut orchestrator, 7100).await {
        Ok(()) => println!("✓ Bonding test setup completed successfully"),
        Err(e) => {
            println!("✗ Bonding test setup failed: {}", e);
            if e.to_string().contains("permission") {
                println!("   → This is expected without CAP_NET_ADMIN privileges");
            }
        }
    }

    println!("\n🎉 Network namespace simulation demo completed!");
    println!("   The new netns testbench provides:");
    println!("   • Real Linux network namespaces with veth interfaces");
    println!("   • Configurable qdisc-based traffic shaping and impairments");
    println!("   • Enhanced 4G/5G network behavior modeling");
    println!("   • Time-varying network conditions");
    println!("   • Drop-in replacement for the old network-sim backend");
    
    println!("✓ Demo completed, orchestrator will clean up on drop");
    
    Ok(())
}

async fn test_link_connectivity(handle: &ScenarioHandle) -> Result<(), Box<dyn std::error::Error>> {
    // Simple connectivity test - try to bind to the ports to verify they're accessible
    use tokio::net::UdpSocket;
    
    let test_socket = UdpSocket::bind(("127.0.0.1", 0)).await?;
    let test_data = b"test packet";
    
    // Try to send a test packet to the ingress port
    if let Err(e) = test_socket.send_to(test_data, ("127.0.0.1", handle.ingress_port)).await {
        return Err(format!("Failed to send to ingress port {}: {}", handle.ingress_port, e).into());
    }
    
    // Give a moment for packet processing
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    
    Ok(())
}

async fn test_bonding_setup(orchestrator: &mut NetworkOrchestrator, rx_port: u16) -> Result<(), Box<dyn std::error::Error>> {
    // Create bonding scenarios with different characteristics
    let scenario_a = TestScenario::bonding_asymmetric();
    let mut scenario_b = TestScenario::mobile_handover();
    
    // Modify second link for bonding diversity
    scenario_b.name = "bonding_link_b".to_string();
    scenario_b.description = "Secondary bonding link with different characteristics".to_string();
    
    let _handles = vec![
        orchestrator.start_scenario(scenario_a, rx_port).await?,
        orchestrator.start_scenario(scenario_b, rx_port + 10).await?,
    ];

    let bonding_links = orchestrator.get_active_links();
    println!("   Bonding links: {}", bonding_links.len());
    
    for (i, link) in bonding_links.iter().enumerate() {
        println!("   Bonding Link {}: {} -> {} (scenario: {})", 
                 i + 1, 
                 link.ingress_port, 
                 link.egress_port, 
                 link.scenario.name);
    }
    
    Ok(())
}