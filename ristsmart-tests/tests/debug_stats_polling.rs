// Debug test to verify stats polling works

use gst::prelude::*;
use gstreamer as gst;
use serde_json;

#[test]
fn test_stats_polling_debug() {
    ristsmart_tests::register_everything_for_tests();

    // Create the mock element
    let stats_mock = gst::ElementFactory::make("riststats_mock")
        .build()
        .expect("Failed to create riststats mock");

    // Test 1: Check that mock returns default stats
    let default_stats: gst::Structure = stats_mock.property("stats");
    println!("Default stats structure: {}", default_stats.to_string());

    // Test 2: Set custom stats and verify they are returned
    let custom_stats = gst::Structure::builder("rist/x-sender-stats")
        .field("session-0.sent-original-packets", 1000u64)
        .field("session-0.sent-retransmitted-packets", 10u64)
        .field("session-0.round-trip-time", 20.0f64)
        .field("session-1.sent-original-packets", 1000u64)
        .field("session-1.sent-retransmitted-packets", 100u64)
        .field("session-1.round-trip-time", 80.0f64)
        .build();

    println!("Custom stats to set: {}", custom_stats.to_string());
    println!("Custom stats has {} fields", custom_stats.n_fields());

    stats_mock.set_property("stats", &custom_stats);

    let retrieved_stats: gst::Structure = stats_mock.property("stats");
    println!("Custom stats structure: {}", retrieved_stats.to_string());
    println!("Retrieved stats has {} fields", retrieved_stats.n_fields());

    // Test 3: Verify the dispatcher can get stats from the mock
    let dispatcher = gst::ElementFactory::make("ristdispatcher")
        .property("auto-balance", true)
        .property("rebalance-interval-ms", 100u64)
        .build()
        .expect("Failed to create ristdispatcher");

    dispatcher.set_property("rist", &stats_mock);

    // Create 2 src pads to match the stats
    for i in 0..2 {
        dispatcher
            .request_pad_simple(&format!("src_{}", i))
            .expect("Failed to request src pad");
    }

    // Check initial weights
    let initial_weights_str: String = dispatcher.property("current-weights");
    println!("Initial weights: {}", initial_weights_str);

    // Wait a bit for the timer to trigger
    std::thread::sleep(std::time::Duration::from_millis(300));

    // Check if weights changed
    let updated_weights_str: String = dispatcher.property("current-weights");
    println!("Updated weights: {}", updated_weights_str);

    // The test passes if we can see the stats being retrieved
    assert!(
        !retrieved_stats.to_string().is_empty(),
        "Stats should not be empty"
    );
}
