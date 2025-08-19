//! Integration tests demonstrating consolidated network simulation and RIST testing

use gstristsmart::testing;
use gstreamer::prelude::*;
use netlink_sim::{TestScenario, NetworkOrchestrator, start_rist_bonding_test};
use serde_json::json;

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

// Helper to build sender pipeline with two RIST sessions
fn build_sender_pipeline(ports: (u16, u16)) -> (gstreamer::Pipeline, gstreamer::Element) {
    let pipeline = gstreamer::Pipeline::new();
    let src = gstreamer::ElementFactory::make("audiotestsrc")
        .property("is-live", true)
        .build()
        .expect("Failed to create audiotestsrc");
    let dispatcher = testing::create_dispatcher_for_testing(Some(&[0.5, 0.5]));
    let sessions = json!([
        {"address": "127.0.0.1", "port": ports.0},
        {"address": "127.0.0.1", "port": ports.1},
    ]);
    let ristsink = gstreamer::ElementFactory::make("ristsink")
        .property("sessions", sessions.to_string())
        .build()
        .expect("Failed to create ristsink");

    pipeline
        .add_many([&src, &dispatcher, &ristsink])
        .expect("failed to add elements");
    gstreamer::Element::link_many([&src, &dispatcher, &ristsink])
        .expect("failed to link sender pipeline");

    (pipeline, ristsink)
}

// Helper to build receiver pipeline listening on rx_port
fn build_receiver_pipeline(rx_port: u16) -> (gstreamer::Pipeline, gstreamer::Element) {
    let pipeline = gstreamer::Pipeline::new();
    let ristsrc = gstreamer::ElementFactory::make("ristsrc")
        .property("address", "0.0.0.0")
        .property("port", rx_port as u32)
        .build()
        .expect("Failed to create ristsrc");
    let counter = testing::create_counter_sink();

    pipeline
        .add_many([&ristsrc, &counter])
        .expect("failed to add receiver elements");
    ristsrc
        .link(&counter)
        .expect("failed to link receiver pipeline");

    (pipeline, counter)
}

// Run two pipelines concurrently for duration seconds
async fn run_pipelines(
    sender: gstreamer::Pipeline,
    receiver: gstreamer::Pipeline,
    duration: u64,
) {
    let s_handle = tokio::task::spawn_blocking(move || {
        testing::run_pipeline_for_duration(&sender, duration).unwrap();
    });
    let r_handle = tokio::task::spawn_blocking(move || {
        testing::run_pipeline_for_duration(&receiver, duration).unwrap();
    });
    let _ = tokio::join!(s_handle, r_handle);
}

/// End-to-end test using real RIST transport elements over simulated links
#[tokio::test]
#[ignore]
async fn test_end_to_end_rist_over_simulated_links() {
    testing::init_for_tests();

    let rx_port = 6000u16;
    let mut orchestrator = NetworkOrchestrator::new(0);
    let link1 = orchestrator
        .start_scenario(TestScenario::baseline_good(), rx_port)
        .await
        .expect("failed to start first link");
    let link2 = orchestrator
        .start_scenario(TestScenario::degraded_network(), rx_port)
        .await
        .expect("failed to start second link");

    let (sender_pipeline, ristsink) = build_sender_pipeline((link1.ingress_port, link2.ingress_port));
    let (receiver_pipeline, counter) = build_receiver_pipeline(rx_port);

    run_pipelines(sender_pipeline, receiver_pipeline, 2).await;

    let count: u64 = testing::get_property(&counter, "count").unwrap();
    assert!(count > 0, "counter_sink received no buffers");

    // Verify that both sessions sent packets if stats are available
    if let Ok(stats) = testing::get_property::<gstreamer::Structure>(&ristsink, "stats") {
        if let Ok(val) = stats.get::<gstreamer::glib::Value>("session-stats") {
            if let Ok(arr) = val.get::<gstreamer::glib::ValueArray>() {
                assert_eq!(arr.len(), 2, "expected stats for two sessions");
                for session in arr.iter() {
                    if let Ok(s) = session.get::<gstreamer::Structure>() {
                        let sent = s.get::<u64>("sent-original-packets").unwrap_or(0);
                        assert!(sent > 0, "session sent no packets");
                    }
                }
            }
        }
    }
}
