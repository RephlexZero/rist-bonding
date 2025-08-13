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
        .property("strategy", "ewma") // Explicitly set EWMA strategy
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

    // Initialize the mock with 2 sessions
    let mock: ristsmart_tests::RistStatsMock = stats_mock.clone().downcast().unwrap();
    mock.set_sessions(2);

    // Set the rist property to point to our stats mock
    dispatcher.set_property("rist", &stats_mock);

    // Create src pads
    for i in 0..2 {
        dispatcher
            .request_pad_simple(&format!("src_{}", i))
            .expect("Failed to request src pad");
    }

    // Establish baseline first
    mock.tick(&[100, 100], &[2, 2], &[40, 40]);
    std::thread::sleep(std::time::Duration::from_millis(150));

    // Now set the stats with dramatic performance difference
    mock.tick(&[2000, 2000], &[5, 400], &[15, 120]); // Session 0: 0.25% loss, 15ms RTT; Session 1: 20% loss, 120ms RTT

    // Create a main loop and run it to allow timer callbacks
    let main_loop = glib::MainLoop::new(None, false);
    let main_loop_clone = main_loop.clone();

    // Set up a timeout to quit the main loop after 800ms
    glib::timeout_add_once(std::time::Duration::from_millis(800), move || {
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

    // The total weight should be normalized to 1.0 for the dispatcher
    let total_weight: f64 = current_weights.iter().sum();
    assert!(
        (total_weight - 1.0).abs() < 0.01,
        "Total weight ({:.3}) should be normalized to 1.0",
        total_weight
    );
}

/// Test AIMD behavior - multiplicative decrease on high loss, additive increase on good performance
#[test]
fn test_aimd_response() {
    ristsmart_tests::register_everything_for_tests();

    let dispatcher = gst::ElementFactory::make("ristdispatcher")
        .property("auto-balance", true)
        .property("rebalance-interval-ms", 100u64) // Use minimum allowed value
        .property("strategy", "aimd") // Use AIMD strategy for this test
        .build()
        .expect("Failed to create ristdispatcher");

    let stats_mock = gst::ElementFactory::make("riststats_mock")
        .build()
        .expect("Failed to create riststats mock");

    // Initialize the mock with 2 sessions
    let mock: ristsmart_tests::RistStatsMock = stats_mock.clone().downcast().unwrap();
    mock.set_sessions(2);

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

    // Establish baseline
    mock.tick(&[100, 100], &[2, 2], &[40, 40]);
    std::thread::sleep(std::time::Duration::from_millis(150));

    // First, simulate very high loss on session 1 (should trigger multiplicative decrease)
    mock.tick(&[1000, 1000], &[5, 500], &[25, 200]); // Session 0: 0.5% loss; Session 1: 50% loss

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
        weights_after_high_loss[0] >= 0.5, // Should be maintained/increased
        "Session 0 weight should be maintained or increased, got {:.3}",
        weights_after_high_loss[0]
    );

    // Now improve session 1's performance (should trigger additive increase over time)
    mock.tick(&[1000, 1000], &[10, 20], &[25, 30]); // Both sessions now have good performance

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

    // Initialize the mock with 2 sessions
    let mock: ristsmart_tests::RistStatsMock = stats_mock.clone().downcast().unwrap();
    mock.set_sessions(2);

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

    // Establish baseline first
    mock.tick(&[100, 100], &[2, 2], &[40, 40]);
    std::thread::sleep(std::time::Duration::from_millis(150));

    // Provide identical, good performance stats for both sessions using mock.tick
    mock.tick(&[1000, 1000], &[20, 20], &[40, 40]); // Both sessions: 2% loss, 40ms RTT

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
