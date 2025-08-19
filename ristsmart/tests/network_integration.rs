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

    // Start two simulated links delivering to a common receiver port.
    let rx_port = 5000u16;
    let mut orchestrator = NetworkOrchestrator::new(0);
    let scenarios = vec![TestScenario::baseline_good(), TestScenario::baseline_good()];
    let handles = orchestrator
        .start_bonding_scenarios(scenarios, rx_port)
        .await
        .expect("failed to start scenarios");
    let ingress_ports: Vec<u16> = handles.iter().map(|h| h.ingress_port).collect();

    let (sender_pipeline, ristsink) = build_sender_pipeline(&ingress_ports);
    let (receiver_pipeline, counter) = build_receiver_pipeline(rx_port);

    run_pipelines(sender_pipeline, receiver_pipeline, 2).await;

    let count: u64 = testing::get_property(&counter, "count").unwrap();
    assert!(count > 0, "counter_sink received no buffers");

    // Verify that both sessions sent packets if stats are available.
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
