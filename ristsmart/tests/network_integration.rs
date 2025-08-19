//! End-to-end RIST bonding test over the network simulator.

use gstreamer::prelude::*;
use gstristsmart::testing::{self, network_sim::*};
use netlink_sim::{NetworkOrchestrator, TestScenario};

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
    )
    .await;

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
    )
    .await;

    assert!(
        result.is_ok(),
        "Test with poor network failed: {:?}",
        result
    );
}

#[tokio::test]
async fn test_bonding_setup() {
    let rx_port = 5006;

    let result = setup_bonding_test(rx_port).await;
    assert!(result.is_ok(), "Bonding setup failed: {:?}", result.err());

    let orchestrator = result.unwrap();
    let active_links = orchestrator.get_active_links();

    // Should have 2 links for bonding
    assert_eq!(
        active_links.len(),
        2,
        "Expected 2 active links for bonding test"
    );

    // Verify different ingress ports
    assert_ne!(
        active_links[0].ingress_port, active_links[1].ingress_port,
        "Links should have different ingress ports"
    );
}

#[tokio::test]
async fn test_multiple_scenarios() {
    let scenarios = get_test_scenarios();

    assert!(
        scenarios.len() >= 4,
        "Expected at least 4 predefined scenarios"
    );

    // Test each scenario setup (without running full tests)
    for (i, scenario) in scenarios.into_iter().enumerate().take(3) {
        let rx_port = 5030 + i as u16; // Use different port range to avoid conflicts
        let result = setup_network_scenario(scenario, rx_port).await;
        assert!(
            result.is_ok(),
            "Failed to setup scenario {}: {:?}",
            i,
            result.err()
        );
    }
}

#[tokio::test]
async fn test_cellular_network_scenario() {
    let scenario = TestScenario::mobile_network();
    let rx_port = 5020;

    let result = setup_network_scenario(scenario, rx_port).await;
    assert!(
        result.is_ok(),
        "Failed to setup cellular scenario: {:?}",
        result.err()
    );
}

// Helper to build sender pipeline with video and audio streams multiplexed into RIST
fn build_sender_pipeline(ports: (u16, u16)) -> (gstreamer::Pipeline, gstreamer::Element) {
    use gstreamer::Caps;
    use gstreamer::Fraction;

    let pipeline = gstreamer::Pipeline::new();

    // --- Video branch ---
    let v_src = gstreamer::ElementFactory::make("videotestsrc")
        .property("is-live", true)
        .build()
        .expect("Failed to create videotestsrc");
    let v_convert = gstreamer::ElementFactory::make("videoconvert")
        .build()
        .expect("Failed to create videoconvert");
    let v_scale = gstreamer::ElementFactory::make("videoscale")
        .build()
        .expect("Failed to create videoscale");
    let v_caps = Caps::builder("video/x-raw")
        .field("width", 1920)
        .field("height", 1080)
        .field("framerate", Fraction::new(60, 1))
        .build();
    let v_capsfilter = gstreamer::ElementFactory::make("capsfilter")
        .property("caps", &v_caps)
        .build()
        .expect("Failed to create video capsfilter");
    let v_enc = gstreamer::ElementFactory::make("x265enc")
        .build()
        .expect("Failed to create x265enc");
    let v_pay = gstreamer::ElementFactory::make("rtph265pay")
        .property("pt", 96u32)
        .build()
        .expect("Failed to create rtph265pay");
    let v_queue = gstreamer::ElementFactory::make("queue")
        .build()
        .expect("Failed to create video queue");

    // --- Audio branch ---
    let a_src = gstreamer::ElementFactory::make("audiotestsrc")
        .property("is-live", true)
        .property("freq", 440.0f64)
        .build()
        .expect("Failed to create audiotestsrc");
    let a_convert = gstreamer::ElementFactory::make("audioconvert")
        .build()
        .expect("Failed to create audioconvert");
    let a_resample = gstreamer::ElementFactory::make("audioresample")
        .build()
        .expect("Failed to create audioresample");
    let a_caps = Caps::builder("audio/x-raw")
        .field("format", "S16BE")
        .field("layout", "interleaved")
        .field("channels", 2)
        .field("rate", 48_000)
        .build();
    let a_capsfilter = gstreamer::ElementFactory::make("capsfilter")
        .property("caps", &a_caps)
        .build()
        .expect("Failed to create audio capsfilter");
    let a_pay = gstreamer::ElementFactory::make("rtpL16pay")
        .property("pt", 97u32)
        .build()
        .expect("Failed to create rtpL16pay");
    let a_queue = gstreamer::ElementFactory::make("queue")
        .build()
        .expect("Failed to create audio queue");

    // --- Mux and sink ---
    let mux = gstreamer::ElementFactory::make("rtpmux")
        .build()
        .expect("Failed to create rtpmux");
    let bonding_addresses = format!("127.0.0.1:{},127.0.0.1:{}", ports.0, ports.1);
    let ristsink = gstreamer::ElementFactory::make("ristsink")
        .property("bonding-addresses", bonding_addresses)
        .build()
        .expect("Failed to create ristsink");

    pipeline
        .add_many([
            &v_src,
            &v_convert,
            &v_scale,
            &v_capsfilter,
            &v_enc,
            &v_pay,
            &v_queue,
            &a_src,
            &a_convert,
            &a_resample,
            &a_capsfilter,
            &a_pay,
            &a_queue,
            &mux,
            &ristsink,
        ])
        .expect("failed to add elements");

    // Link video branch
    gstreamer::Element::link_many(&[
        &v_src,
        &v_convert,
        &v_scale,
        &v_capsfilter,
        &v_enc,
        &v_pay,
        &v_queue,
    ])
    .expect("failed to link video branch");
    v_queue
        .link(&mux)
        .expect("failed to link video queue to mux");

    // Link audio branch
    gstreamer::Element::link_many(&[
        &a_src,
        &a_convert,
        &a_resample,
        &a_capsfilter,
        &a_pay,
        &a_queue,
    ])
    .expect("failed to link audio branch");
    a_queue
        .link(&mux)
        .expect("failed to link audio queue to mux");

    // Link mux to sink
    mux.link(&ristsink).expect("failed to link mux to ristsink");

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

// Run two pipelines concurrently for longer duration to allow jitter buffer
async fn run_pipelines(sender: gstreamer::Pipeline, receiver: gstreamer::Pipeline) {
    receiver.set_state(gstreamer::State::Playing).unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    sender.set_state(gstreamer::State::Playing).unwrap();
    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await; // Increased from 2 to 10 seconds

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
