//! Integration tests demonstrating consolidated network simulation and RIST testing

use gstristsmart::testing;
use gstreamer::prelude::*;
use netlink_sim::{TestScenario, NetworkOrchestrator, start_rist_bonding_test};

// Helper functions for network simulation integration
async fn setup_network_scenario(scenario: TestScenario, rx_port: u16) -> Result<NetworkOrchestrator, Box<dyn std::error::Error>> {
    // Seed the orchestrator based on the rx_port so multiple orchestrators
    // started during tests allocate different port ranges and don't collide.
    let seed = rx_port as u64 + 1000;
    let mut orchestrator = NetworkOrchestrator::new(seed);
    let _handle = orchestrator.start_scenario(scenario, rx_port).await?;
    Ok(orchestrator)
}

async fn setup_bonding_test(rx_port: u16) -> Result<NetworkOrchestrator, Box<dyn std::error::Error>> {
    let orchestrator = start_rist_bonding_test(rx_port).await?;
    Ok(orchestrator)
}

async fn test_dispatcher_with_network(
    weights: Option<&[f64]>,
    scenario: TestScenario,
    rx_port: u16,
    test_duration_secs: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let _orchestrator = setup_network_scenario(scenario, rx_port).await?;
    
    // Initialize RIST elements
    testing::init_for_tests();
    
    let dispatcher = testing::create_dispatcher_for_testing(weights);
    
    // Create a simple test pipeline
    let pipeline = gstreamer::Pipeline::new();
    let src = testing::create_test_source();
    let sink = testing::create_fake_sink();
    
    pipeline.add_many([&src, &dispatcher, &sink])?;
    gstreamer::Element::link_many([&src, &dispatcher, &sink])?;
    
    // Run the test
    testing::run_pipeline_for_duration(&pipeline, test_duration_secs)?;
    
    Ok(())
}

fn get_test_scenarios() -> Vec<TestScenario> {
    vec![
        TestScenario::baseline_good(),
        TestScenario::degraded_network(),
        TestScenario::mobile_network(),
        TestScenario::bonding_asymmetric(),
        TestScenario::varying_quality(),
    ]
}

#[tokio::test]
async fn test_dispatcher_with_good_network() {
    testing::init_for_tests();
    
    let scenario = TestScenario::baseline_good();
    let rx_port = 5004;
    
    let result = test_dispatcher_with_network(
        Some(&[0.6, 0.4]),
        scenario,
        rx_port,
        5, // 5 second test
    ).await;
    
    assert!(result.is_ok(), "Test failed: {:?}", result);
}

#[tokio::test] 
async fn test_dispatcher_with_poor_network() {
    testing::init_for_tests();
    
    let scenario = TestScenario::degraded_network();
    let rx_port = 5005;
    
    let result = test_dispatcher_with_network(
        Some(&[0.5, 0.5]),
        scenario,
        rx_port,
        5, // 5 second test
    ).await;
    
    assert!(result.is_ok(), "Test with poor network failed: {:?}", result);
}

#[tokio::test]
async fn test_bonding_setup() {
    let rx_port = 5006;
    
    let result = setup_bonding_test(rx_port).await;
    assert!(result.is_ok(), "Bonding setup failed: {:?}", result.err());
    
    let orchestrator = result.unwrap();
    let active_links = orchestrator.get_active_links();
    
    // Should have 2 links for bonding
    assert_eq!(active_links.len(), 2, "Expected 2 active links for bonding test");
    
    // Verify different ingress ports
    assert_ne!(active_links[0].ingress_port, active_links[1].ingress_port, 
        "Links should have different ingress ports");
}

#[tokio::test] 
async fn test_multiple_scenarios() {
    let scenarios = get_test_scenarios();
    
    assert!(scenarios.len() >= 4, "Expected at least 4 predefined scenarios");
    
    // Test each scenario setup (without running full tests)
    for (i, scenario) in scenarios.into_iter().enumerate().take(3) {
        let rx_port = 5030 + i as u16; // Use different port range to avoid conflicts
        let result = setup_network_scenario(scenario, rx_port).await;
        assert!(result.is_ok(), "Failed to setup scenario {}: {:?}", i, result.err());
    }
}

#[tokio::test]
async fn test_cellular_network_scenario() {
    let scenario = TestScenario::mobile_network();
    let rx_port = 5020;
    
    let result = setup_network_scenario(scenario, rx_port).await;
    assert!(result.is_ok(), "Failed to setup cellular scenario: {:?}", result.err());
}
