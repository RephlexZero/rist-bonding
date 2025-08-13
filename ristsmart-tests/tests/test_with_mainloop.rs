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

    // Create src pads first
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

    // Create a main loop and run it for a longer time to allow more timer callbacks
    let main_loop = glib::MainLoop::new(None, false);
    let main_loop_clone = main_loop.clone();

    // Set up a timeout to quit the main loop after 800ms (allow for more updates)
    glib::timeout_add_once(Duration::from_millis(800), move || {
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
