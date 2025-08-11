// EWMA test with proper main loop

use gst::prelude::*;
use gstreamer as gst;
use serde_json;
use std::time::Duration;

/// Test EWMA weight updates with a proper main loop
#[test]
fn test_ewma_with_mainloop() {
    ristsmart_tests::register_everything_for_tests();

    let dispatcher = gst::ElementFactory::make("ristdispatcher")
        .property("auto-balance", true)
        .property("rebalance-interval-ms", 100u64) // Fast updates for testing
        .build()
        .expect("Failed to create ristdispatcher");

    // Start with equal weights
    let initial_weights = vec![1.0, 1.0];
    let weights_json = serde_json::to_string(&initial_weights).unwrap();
    dispatcher.set_property("weights", &weights_json);

    // Create a mock RIST stats provider
    let stats_mock = gst::ElementFactory::make("riststats_mock")
        .build()
        .expect("Failed to create riststats mock");

    // Set the rist property to point to our stats mock
    dispatcher.set_property("rist", &stats_mock);

    // Create src pads
    for i in 0..2 {
        dispatcher
            .request_pad_simple(&format!("src_{}", i))
            .expect("Failed to request src pad");
    }

    // Simulate better performance on session 0
    let good_stats = gst::Structure::builder("rist/x-sender-stats")
        .field("session-0.sent-original-packets", 1000u64)
        .field("session-0.sent-retransmitted-packets", 10u64) // Low retransmission rate
        .field("session-0.round-trip-time", 20.0f64) // Low RTT
        .field("session-1.sent-original-packets", 1000u64)
        .field("session-1.sent-retransmitted-packets", 100u64) // High retransmission rate
        .field("session-1.round-trip-time", 80.0f64) // High RTT
        .build();

    stats_mock.set_property("stats", &good_stats);

    // Create a main loop and run it for a short time to allow timer callbacks
    let main_loop = glib::MainLoop::new(None, false);
    let main_loop_clone = main_loop.clone();

    // Set up a timeout to quit the main loop after 300ms
    glib::timeout_add_once(Duration::from_millis(300), move || {
        main_loop_clone.quit();
    });

    // Run the main loop
    main_loop.run();

    // Check if weights have adapted
    let current_weights_str: String = dispatcher.property("current-weights");
    let current_weights: Vec<f64> =
        serde_json::from_str(&current_weights_str).expect("Failed to parse current weights");

    println!("Initial weights: {:?}", initial_weights);
    println!("Adapted weights: {:?}", current_weights);

    // Session 0 should have higher weight due to better performance
    assert!(
        current_weights[0] > current_weights[1],
        "Expected session 0 weight ({:.3}) > session 1 weight ({:.3})",
        current_weights[0],
        current_weights[1]
    );
}
