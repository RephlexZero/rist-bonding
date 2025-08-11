// EWMA/AIMD weight adaptation unit tests

use gstreamer::{self as gst, prelude::*};
use serde_json;

/// Test EWMA weight updates respond correctly to synthetic performance data
#[test]
fn test_ewma_adaptation() {
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

    // Simulate better performance on session 0 by providing synthetic stats
    // where session 0 has lower RTX rate and RTT
    let good_stats = gst::Structure::builder("rist/x-sender-stats")
        .field("session-0.sent-original-packets", 1000u64)
        .field("session-0.sent-retransmitted-packets", 10u64) // Low retransmission rate
        .field("session-0.round-trip-time", 20.0f64) // Low RTT
        .field("session-1.sent-original-packets", 1000u64)
        .field("session-1.sent-retransmitted-packets", 100u64) // High retransmission rate
        .field("session-1.round-trip-time", 80.0f64) // High RTT
        .build();

    stats_mock.set_property("stats", &good_stats);

    // Wait for a few rebalance intervals to let EWMA adapt using main loop
    let main_loop = glib::MainLoop::new(None, false);
    let main_loop_clone = main_loop.clone();

    // Set up a timeout to quit the main loop after 500ms
    glib::timeout_add_once(std::time::Duration::from_millis(500), move || {
        main_loop_clone.quit();
    });

    // Run the main loop to allow timer callbacks
    main_loop.run();

    // Check that weights have adapted - session 0 should have higher weight
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

    // The total weight should be reasonable (around 2.0 for two sessions)
    let total_weight: f64 = current_weights.iter().sum();
    assert!(
        total_weight > 1.5 && total_weight < 3.0,
        "Total weight ({:.3}) should be reasonable",
        total_weight
    );
}

/// Test AIMD behavior - multiplicative decrease on high loss, additive increase on good performance
#[test]
fn test_aimd_response() {
    ristsmart_tests::register_everything_for_tests();

    let dispatcher = gst::ElementFactory::make("ristdispatcher")
        .property("auto-balance", true)
        .property("rebalance-interval-ms", 50u64) // Very fast updates
        .build()
        .expect("Failed to create ristdispatcher");

    let stats_mock = gst::ElementFactory::make("riststats_mock")
        .build()
        .expect("Failed to create riststats mock");

    dispatcher.set_property("rist", &stats_mock);

    // Start with equal weights
    let initial_weights = vec![1.0, 1.0];
    let weights_json = serde_json::to_string(&initial_weights).unwrap();
    dispatcher.set_property("weights", &weights_json);

    // Create src pads
    for i in 0..2 {
        dispatcher
            .request_pad_simple(&format!("src_{}", i))
            .expect("Failed to request src pad");
    }

    // First, simulate very high loss on session 1 (should trigger multiplicative decrease)
    let high_loss_stats = gst::Structure::builder("rist/x-sender-stats")
        .field("session-0.sent-original-packets", 1000u64)
        .field("session-0.sent-retransmitted-packets", 5u64) // Good performance
        .field("session-0.round-trip-time", 25.0f64)
        .field("session-1.sent-original-packets", 1000u64)
        .field("session-1.sent-retransmitted-packets", 500u64) // 50% retransmission rate!
        .field("session-1.round-trip-time", 200.0f64) // High RTT
        .build();

    stats_mock.set_property("stats", &high_loss_stats);

    // Wait for adaptation using main loop
    let main_loop = glib::MainLoop::new(None, false);
    let main_loop_clone = main_loop.clone();

    glib::timeout_add_once(std::time::Duration::from_millis(200), move || {
        main_loop_clone.quit();
    });

    main_loop.run();

    let weights_after_high_loss: Vec<f64> =
        serde_json::from_str(&dispatcher.property::<String>("current-weights"))
            .expect("Failed to parse weights");

    println!("Weights after high loss: {:?}", weights_after_high_loss);

    // Session 1 should have much lower weight due to high loss (AIMD multiplicative decrease)
    assert!(
        weights_after_high_loss[1] < 0.5, // Should be significantly reduced
        "Session 1 weight should be reduced due to high loss, got {:.3}",
        weights_after_high_loss[1]
    );

    // Session 0 should maintain or increase weight
    assert!(
        weights_after_high_loss[0] >= 0.8, // Should be maintained/increased
        "Session 0 weight should be maintained or increased, got {:.3}",
        weights_after_high_loss[0]
    );

    // Now improve session 1's performance (should trigger additive increase over time)
    let improved_stats = gst::Structure::builder("rist/x-sender-stats")
        .field("session-0.sent-original-packets", 2000u64)
        .field("session-0.sent-retransmitted-packets", 10u64) // Still good
        .field("session-0.round-trip-time", 25.0f64)
        .field("session-1.sent-original-packets", 2000u64)
        .field("session-1.sent-retransmitted-packets", 20u64) // Much improved!
        .field("session-1.round-trip-time", 30.0f64) // Much better RTT
        .build();

    stats_mock.set_property("stats", &improved_stats);

    // Wait longer for additive increase to take effect using main loop
    let main_loop = glib::MainLoop::new(None, false);
    let main_loop_clone = main_loop.clone();

    glib::timeout_add_once(std::time::Duration::from_millis(400), move || {
        main_loop_clone.quit();
    });

    main_loop.run();

    let final_weights: Vec<f64> =
        serde_json::from_str(&dispatcher.property::<String>("current-weights"))
            .expect("Failed to parse final weights");

    println!("Final weights after improvement: {:?}", final_weights);

    // Session 1 weight should have increased from its low point (additive increase)
    assert!(
        final_weights[1] > weights_after_high_loss[1],
        "Session 1 weight should recover with additive increase: {:.3} -> {:.3}",
        weights_after_high_loss[1],
        final_weights[1]
    );
}

/// Test weight convergence with stable performance
#[test]
fn test_ewma_convergence() {
    ristsmart_tests::register_everything_for_tests();

    let dispatcher = gst::ElementFactory::make("ristdispatcher")
        .property("auto-balance", true)
        .property("rebalance-interval-ms", 100u64)
        .build()
        .expect("Failed to create ristdispatcher");

    let stats_mock = gst::ElementFactory::make("riststats_mock")
        .build()
        .expect("Failed to create riststats mock");

    dispatcher.set_property("rist", &stats_mock);

    // Start with unequal weights
    let initial_weights = vec![0.3, 1.7];
    let weights_json = serde_json::to_string(&initial_weights).unwrap();
    dispatcher.set_property("weights", &weights_json);

    // Create src pads
    for i in 0..2 {
        dispatcher
            .request_pad_simple(&format!("src_{}", i))
            .expect("Failed to request src pad");
    }

    // Provide identical, good performance stats for both sessions
    let equal_stats = gst::Structure::builder("rist/x-sender-stats")
        .field("session-0.sent-original-packets", 1000u64)
        .field("session-0.sent-retransmitted-packets", 20u64) // 2% loss
        .field("session-0.round-trip-time", 40.0f64)
        .field("session-1.sent-original-packets", 1000u64)
        .field("session-1.sent-retransmitted-packets", 20u64) // 2% loss
        .field("session-1.round-trip-time", 40.0f64)
        .build();

    stats_mock.set_property("stats", &equal_stats);

    // Wait for convergence using main loop
    let main_loop = glib::MainLoop::new(None, false);
    let main_loop_clone = main_loop.clone();

    glib::timeout_add_once(std::time::Duration::from_millis(800), move || {
        main_loop_clone.quit();
    });

    main_loop.run();

    let final_weights: Vec<f64> =
        serde_json::from_str(&dispatcher.property::<String>("current-weights"))
            .expect("Failed to parse final weights");

    println!("Initial weights: {:?}", initial_weights);
    println!("Converged weights: {:?}", final_weights);

    // With identical performance, weights should converge toward equal values
    let weight_diff = (final_weights[0] - final_weights[1]).abs();
    assert!(
        weight_diff < 0.3, // Allow some tolerance
        "Weights should converge with equal performance, diff: {:.3}",
        weight_diff
    );

    // Both weights should be reasonable (not too extreme)
    for weight in &final_weights {
        assert!(
            *weight > 0.2 && *weight < 2.0,
            "Individual weights should be reasonable, got {:.3}",
            weight
        );
    }
}
