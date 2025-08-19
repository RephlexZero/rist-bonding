//! End-to-end RIST bonding test over the network simulator.

use gstreamer::prelude::*;
use gstristsmart::testing;
use netlink_sim::{NetworkOrchestrator, TestScenario};
use serde_json::json;

/// Create a sender pipeline with sessions pointing to orchestrator ingress ports.
fn build_sender_pipeline(ports: &[u16]) -> (gstreamer::Pipeline, gstreamer::Element) {
    let pipeline = gstreamer::Pipeline::new();
    let src = gstreamer::ElementFactory::make("audiotestsrc")
        .property("is-live", true)
        .build()
        .expect("Failed to create audiotestsrc");
    let dispatcher = testing::create_dispatcher_for_testing(Some(&[0.5, 0.5]));

    let sessions: Vec<_> = ports
        .iter()
        .map(|p| json!({"address": "127.0.0.1", "port": p}))
        .collect();
    let ristsink = gstreamer::ElementFactory::make("ristsink")
        .property("bonding-addresses", sessions.to_string())
        .build()
        .expect("Failed to create ristsink");

    pipeline
        .add_many([&src, &dispatcher, &ristsink])
        .expect("failed to add elements");
    gstreamer::Element::link_many([&src, &dispatcher, &ristsink])
        .expect("failed to link sender pipeline");

    (pipeline, ristsink)
}

/// Create a receiver pipeline listening on `port`.
fn build_receiver_pipeline(port: u16) -> (gstreamer::Pipeline, gstreamer::Element) {
    let pipeline = gstreamer::Pipeline::new();
    let ristsrc = gstreamer::ElementFactory::make("ristsrc")
        .property("address", "0.0.0.0")
        .property("port", port as u32)
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

/// Run two pipelines concurrently for `duration` seconds.
async fn run_pipelines(sender: gstreamer::Pipeline, receiver: gstreamer::Pipeline, duration: u64) {
    let s_handle = tokio::task::spawn_blocking(move || {
        testing::run_pipeline_for_duration(&sender, duration).unwrap();
    });
    let r_handle = tokio::task::spawn_blocking(move || {
        testing::run_pipeline_for_duration(&receiver, duration).unwrap();
    });
    let _ = tokio::join!(s_handle, r_handle);
}

/// End-to-end test using real RIST transport elements over simulated links.
#[tokio::test]
async fn test_end_to_end_rist_over_simulated_network() {
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
    
    // Create capsfilter to specify audio format for RTP payloader
    let caps = gstreamer::Caps::builder("audio/x-raw")
        .field("format", "S16BE")
        .field("layout", "interleaved")
        .field("channels", 2)
        .field("rate", 48000)
        .build();
    let capsfilter = gstreamer::ElementFactory::make("capsfilter")
        .property("caps", &caps)
        .build()
        .expect("Failed to create capsfilter");
    
    // Add RTP payloader since ristsink expects RTP payloaded data (per documentation)
    let payloader = gstreamer::ElementFactory::make("rtpL16pay")
        .build()
        .expect("Failed to create rtpL16pay");
    
    // Create ristsink with bonding addresses - it will handle multiple sessions internally
    let bonding_addresses = format!("127.0.0.1:{},127.0.0.1:{}", ports.0, ports.1);
    let ristsink = gstreamer::ElementFactory::make("ristsink")
        .property("bonding-addresses", bonding_addresses)
        .build()
        .expect("Failed to create ristsink");

    pipeline
        .add_many([&src, &capsfilter, &payloader, &ristsink])
        .expect("failed to add elements");
    gstreamer::Element::link_many([&src, &capsfilter, &payloader, &ristsink])
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
    
    // Add RTP depayloader since ristsrc outputs RTP data (per documentation)
    // Use natural caps negotiation without restrictive capsfilter
    let depayloader = gstreamer::ElementFactory::make("rtpL16depay")
        .build()
        .expect("Failed to create rtpL16depay");
    
    let counter = testing::create_counter_sink();

    pipeline
        .add_many([&ristsrc, &depayloader, &counter])
        .expect("failed to add receiver elements");
    gstreamer::Element::link_many([&ristsrc, &depayloader, &counter])
        .expect("failed to link receiver pipeline");

    (pipeline, counter)
}

// Run two pipelines concurrently for longer duration to allow jitter buffer
async fn run_pipelines(
    sender: gstreamer::Pipeline,
    receiver: gstreamer::Pipeline,
) {
    receiver.set_state(gstreamer::State::Playing).unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    sender.set_state(gstreamer::State::Playing).unwrap();
    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;  // Increased from 2 to 10 seconds

    sender.set_state(gstreamer::State::Null).unwrap();
    receiver.set_state(gstreamer::State::Null).unwrap();
}

/// End-to-end test using real RIST transport elements over simulated links
#[tokio::test]
#[ignore]
async fn test_end_to_end_rist_over_simulated_links() {
    testing::init_for_tests();

    let rx_port = 6000u16;
    let mut orchestrator = NetworkOrchestrator::new(8000); // Start from 8000 (even)
    
    // Get first link
    let link1 = orchestrator
        .start_scenario(TestScenario::baseline_good(), rx_port)
        .await
        .expect("failed to start first link");
    
    // Skip to next even port for second link
    let mut link2 = orchestrator
        .start_scenario(TestScenario::degraded_network(), rx_port)
        .await
        .expect("failed to start second link");
    
    // If second link is odd, get another one
    while link2.ingress_port % 2 != 0 {
        link2 = orchestrator
            .start_scenario(TestScenario::degraded_network(), rx_port)
            .await
            .expect("failed to start second link");
    }

    let port1 = link1.ingress_port;
    let port2 = link2.ingress_port;
    
    println!("Using even ingress ports: {} and {}", port1, port2);

    // Run for longer to allow jitter buffer to properly release packets
    let (sender_pipeline, ristsink) = build_sender_pipeline((port1, port2));
    let (receiver_pipeline, counter) = build_receiver_pipeline(rx_port);
    
    run_pipelines(sender_pipeline, receiver_pipeline).await;

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
