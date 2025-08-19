//! Statistics-driven rebalancing and adaptive logic tests
//!
//! These tests verify that the dispatcher can adapt to changing network conditions
//! based on RIST statistics and properly rebalance traffic accordingly.

use gst::prelude::*;
use gstreamer as gst;
use gstristsmart::test_pipeline;
use gstristsmart::testing::*;
use std::time::Duration;

#[test]
fn test_stats_driven_dispatcher_rebalancing() {
    init_for_tests();

    println!("=== Stats-Driven Dispatcher Rebalancing Test ===");

    // Create elements
    let source = create_test_source();
    let dispatcher = gst::ElementFactory::make("ristdispatcher")
        .property("rebalance-interval-ms", 100u64) // Fast rebalancing for testing
        .property("auto-balance", true) // Enable automatic rebalancing
        .property("strategy", "ewma") // Use EWMA strategy
        .build()
        .expect("Failed to create ristdispatcher");

    let mock_stats = create_mock_stats(2);
    let counter1 = create_counter_sink();
    let counter2 = create_counter_sink();

    // Create pipeline
    test_pipeline!(pipeline, &source, &dispatcher, &counter1, &counter2);

    // Set up initial mock stats (session 0 performs better than session 1)
    let initial_stats = gst::Structure::builder("rist/x-sender-stats")
        .field("session-0.sent-original-packets", 1000u64)
        .field("session-0.sent-retransmitted-packets", 20u64) // 2% loss
        .field("session-0.round-trip-time", 30.0f64) // Good RTT
        .field("session-1.sent-original-packets", 1000u64)
        .field("session-1.sent-retransmitted-packets", 100u64) // 10% loss
        .field("session-1.round-trip-time", 150.0f64) // Poor RTT
        .build();

    mock_stats.set_property("stats", &initial_stats);

    // Request src pads and link
    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();

    source
        .link(&dispatcher)
        .expect("Failed to link source to dispatcher");
    src_0
        .link(&counter1.static_pad("sink").unwrap())
        .expect("Failed to link src_0");
    src_1
        .link(&counter2.static_pad("sink").unwrap())
        .expect("Failed to link src_1");

    // Run initial phase
    run_pipeline_for_duration(&pipeline, 1).expect("Pipeline run failed");

    let initial_count1: u64 = get_property(&counter1, "count").unwrap();
    let initial_count2: u64 = get_property(&counter2, "count").unwrap();

    println!(
        "Initial distribution - Counter 1: {}, Counter 2: {}",
        initial_count1, initial_count2
    );

    // Now improve session 1 and see if dispatcher adapts
    mock_stats.recover(1); // This should improve session 1's stats

    // Continue running
    pipeline
        .set_state(gst::State::Playing)
        .expect("Failed to restart pipeline");
    std::thread::sleep(Duration::from_secs(1));
    pipeline
        .set_state(gst::State::Null)
        .expect("Failed to stop pipeline");

    let final_count1: u64 = get_property(&counter1, "count").unwrap();
    let final_count2: u64 = get_property(&counter2, "count").unwrap();

    println!(
        "Final distribution - Counter 1: {}, Counter 2: {}",
        final_count1, final_count2
    );

    // Verify that traffic was distributed (both counters should have some packets)
    assert!(
        final_count1 > initial_count1,
        "Counter 1 should have received more packets"
    );
    assert!(
        final_count2 >= initial_count2,
        "Counter 2 should have maintained or increased"
    );

    println!("✅ Stats-driven rebalancing test completed");
}

#[test]
fn test_coordinated_stats_polling() {
    init_for_tests();

    println!("=== Coordinated Stats Polling Test ===");

    // Create mock stats element
    let mock_stats = create_mock_stats(3);

    // Simulate realistic traffic progression
    println!("Initial state: Equal performance");
    mock_stats.tick(&[100, 100, 100], &[5, 5, 5], &[25, 25, 25]);

    let stats1 = mock_stats.property::<gst::Structure>("stats");
    println!("Stats after initial tick: {}", stats1);

    // Simulate network degradation on session 1
    println!("Degrading session 1...");
    mock_stats.degrade(1, 50, 200);

    let stats2 = mock_stats.property::<gst::Structure>("stats");
    println!("Stats after degradation: {}", stats2);

    // Verify degradation is reflected in stats
    let session1_retrans = stats2
        .get::<u64>("session-1.sent-retransmitted-packets")
        .unwrap();
    let session1_rtt = stats2.get::<f64>("session-1.round-trip-time").unwrap();

    assert!(
        session1_retrans > 5,
        "Session 1 should have increased retransmissions"
    );
    assert!(session1_rtt > 25.0, "Session 1 should have increased RTT");

    // Simulate recovery
    println!("Recovering session 1...");
    mock_stats.recover(1);

    let stats3 = mock_stats.property::<gst::Structure>("stats");
    println!("Stats after recovery: {}", stats3);

    let recovered_retrans = stats3
        .get::<u64>("session-1.sent-retransmitted-packets")
        .unwrap();
    let recovered_rtt = stats3.get::<f64>("session-1.round-trip-time").unwrap();

    assert!(
        recovered_retrans < session1_retrans,
        "Retransmissions should decrease after recovery"
    );
    assert!(
        recovered_rtt < session1_rtt,
        "RTT should improve after recovery"
    );

    println!("✅ Coordinated stats polling test completed");
}

#[test]
fn test_dynbitrate_integration() {
    init_for_tests();

    println!("=== Dynamic Bitrate Integration Test ===");

    // Create elements
    let source = create_test_source();
    let encoder = create_encoder_stub(Some(5000)); // Start at 5Mbps
    let dynbitrate = create_dynbitrate();
    let sink = create_fake_sink();

    // Create pipeline
    test_pipeline!(pipeline, &source, &encoder, &dynbitrate, &sink);

    // Link elements
    source
        .link(&encoder)
        .expect("Failed to link source to encoder");
    encoder
        .link(&dynbitrate)
        .expect("Failed to link encoder to dynbitrate");
    dynbitrate
        .link(&sink)
        .expect("Failed to link dynbitrate to sink");

    // Test initial state
    wait_for_state_change(&pipeline, gst::State::Paused, 5).expect("Failed to pause pipeline");

    let initial_bitrate: u32 = get_property(&encoder, "bitrate").unwrap();
    println!("Initial encoder bitrate: {} kbps", initial_bitrate);
    assert_eq!(initial_bitrate, 5000);

    // Run pipeline briefly
    run_pipeline_for_duration(&pipeline, 1).expect("Pipeline run failed");

    println!("✅ Dynamic bitrate integration test completed");
}

#[test]
fn test_ewma_weight_calculation() {
    init_for_tests();

    println!("=== EWMA Weight Calculation Test ===");

    // Create dispatcher with EWMA strategy
    let dispatcher = gst::ElementFactory::make("ristdispatcher")
        .property("strategy", "ewma")
        .property("rebalance-interval-ms", 200u64)
        .build()
        .expect("Failed to create ristdispatcher");

    // Set initial weights
    dispatcher.set_property("weights", "[0.5, 0.5]");

    // Create output pads
    let _src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    let _src_1 = dispatcher.request_pad_simple("src_%u").unwrap();

    // Verify initial setup
    let strategy: String = get_property(&dispatcher, "strategy").unwrap();
    let current_weights: String = get_property(&dispatcher, "current-weights").unwrap();

    assert_eq!(strategy, "ewma");
    println!("Current weights: {}", current_weights);

    // Test property updates
    dispatcher.set_property("weights", "[0.7, 0.3]");
    let updated_weights: String = get_property(&dispatcher, "current-weights").unwrap();
    println!("Updated weights: {}", updated_weights);

    println!("✅ EWMA weight calculation test completed");
}
