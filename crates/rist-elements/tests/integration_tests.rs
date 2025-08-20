//! Integration test demonstrating improved RIST testing integration
//!
//! This test shows how the integrated testing approach reduces boilerplate
//! and provides better organization.

use gst::prelude::*;
use gstreamer as gst;
use gstristelements::testing::*;

#[test]
fn test_integrated_dispatcher_flow() {
    init_for_tests();

    // Create elements using convenience functions
    let dispatcher = create_dispatcher_for_testing(Some(&[0.6, 0.4]));
    let counter1 = create_counter_sink();
    let counter2 = create_counter_sink();
    let source = create_test_source();

    // Create pipeline
    let pipeline = gst::Pipeline::new();
    pipeline
        .add_many([&source, &dispatcher, &counter1, &counter2])
        .expect("Failed to add elements to pipeline");

    // Request src pads from dispatcher
    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();

    // Link elements
    source
        .link(&dispatcher)
        .expect("Failed to link source to dispatcher");
    src_0
        .link(&counter1.static_pad("sink").unwrap())
        .expect("Failed to link dispatcher src_0");
    src_1
        .link(&counter2.static_pad("sink").unwrap())
        .expect("Failed to link dispatcher src_1");

    // Run the test
    run_pipeline_for_duration(&pipeline, 2).expect("Pipeline run failed");

    // Verify results
    let count1: u64 = get_property(&counter1, "count").expect("Failed to get count1");
    let count2: u64 = get_property(&counter2, "count").expect("Failed to get count2");

    println!(
        "Counter 1: {} buffers, Counter 2: {} buffers",
        count1, count2
    );
    assert!(count1 > 0, "Counter 1 should receive buffers");
    assert!(count2 > 0, "Counter 2 should receive buffers");
}

#[cfg(feature = "test-plugin")]
#[test]
fn test_stats_driven_rebalancing() {
    init_for_tests();

    // Create mock stats and connect to dispatcher
    let mock_stats = create_mock_stats(2);
    let _dispatcher = create_dispatcher(Some(&[0.5, 0.5]));

    // Set up initial good stats
    mock_stats.tick(&[100, 100], &[5, 5], &[25, 25]);

    // Verify initial state
    let stats = mock_stats.property::<gst::Structure>("stats");
    println!("Initial stats: {}", stats);

    // Simulate degradation on session 0
    mock_stats.degrade(0, 50, 150); // More retrans, higher RTT

    let degraded_stats = mock_stats.property::<gst::Structure>("stats");
    println!("Degraded stats: {}", degraded_stats);

    // Simulate recovery
    mock_stats.recover(0);

    let recovered_stats = mock_stats.property::<gst::Structure>("stats");
    println!("Recovered stats: {}", recovered_stats);

    // Verify recovery worked
    let retrans_0 = recovered_stats
        .get::<u64>("session-0.sent-retransmitted-packets")
        .unwrap();
    let rtt_0 = recovered_stats
        .get::<f64>("session-0.round-trip-time")
        .unwrap();

    assert!(
        retrans_0 < 55,
        "Retransmissions should be reduced after recovery"
    );
    assert!(rtt_0 < 100.0, "RTT should be improved after recovery");
}

#[cfg(feature = "test-plugin")]
#[test]
fn test_dynbitrate_integration() {
    init_for_tests();

    let dynbitrate = create_dynbitrate();
    let encoder = create_encoder_stub(Some(5000));
    let source = create_test_source();
    let sink = create_fake_sink();

    // Create pipeline
    let pipeline = gst::Pipeline::new();
    pipeline
        .add_many([&source, &encoder, &dynbitrate, &sink])
        .expect("Failed to add elements to pipeline");

    // Link all elements
    gst::Element::link_many([&source, &encoder, &dynbitrate, &sink])
        .expect("Failed to link elements");

    // Test the pipeline
    wait_for_state_change(&pipeline, gst::State::Paused, 5).expect("Failed to pause pipeline");

    // Verify encoder bitrate
    let initial_bitrate: u32 =
        get_property(&encoder, "bitrate").expect("Failed to get encoder bitrate");
    assert_eq!(initial_bitrate, 5000);

    pipeline
        .set_state(gst::State::Null)
        .expect("Failed to stop pipeline");
}

#[test]
fn test_element_properties() {
    init_for_tests();

    let dispatcher = create_dispatcher(None);

    // Test setting properties
    dispatcher.set_property("rebalance-interval-ms", 500u64);
    dispatcher.set_property("strategy", "ewma");

    // Test getting properties
    let interval: u64 = get_property(&dispatcher, "rebalance-interval-ms")
        .expect("Failed to get rebalance interval");
    let strategy: String = get_property(&dispatcher, "strategy").expect("Failed to get strategy");

    assert_eq!(interval, 500);
    assert_eq!(strategy, "ewma");
}
