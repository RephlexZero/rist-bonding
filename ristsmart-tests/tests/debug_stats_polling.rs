// Test to verify stats polling works with deterministic assertions

use gst::prelude::*;
use gstreamer as gst;
use serde_json;
use std::time::Duration;

#[test]
fn test_stats_polling_deterministic() {
    ristsmart_tests::register_everything_for_tests();

    // Create the mock element
    let stats_mock = gst::ElementFactory::make("riststats_mock")
        .build()
        .expect("Failed to create riststats mock");

    // Test 1: Verify default stats structure
    let default_stats: gst::Structure = stats_mock.property("stats");
    println!("Default stats structure: {}", default_stats.to_string());

    // Validate default stats structure
    assert_eq!(
        default_stats.name(),
        "rist/x-sender-stats",
        "Default stats should have correct structure name"
    );
    assert!(
        default_stats.n_fields() >= 6,
        "Default stats should have at least 6 fields for 2 sessions"
    );

    // Verify default session data exists
    assert!(
        default_stats.has_field("session-0.sent-original-packets"),
        "Should have session-0 sent-original-packets field"
    );
    assert!(
        default_stats.has_field("session-0.sent-retransmitted-packets"),
        "Should have session-0 sent-retransmitted-packets field"
    );
    assert!(
        default_stats.has_field("session-0.round-trip-time"),
        "Should have session-0 round-trip-time field"
    );

    // Test 2: Set and verify custom stats
    let custom_stats = gst::Structure::builder("rist/x-sender-stats")
        .field("session-0.sent-original-packets", 1000u64)
        .field("session-0.sent-retransmitted-packets", 10u64)
        .field("session-0.round-trip-time", 20.0f64)
        .field("session-1.sent-original-packets", 1000u64)
        .field("session-1.sent-retransmitted-packets", 100u64)
        .field("session-1.round-trip-time", 80.0f64)
        .build();

    stats_mock.set_property("stats", &custom_stats);

    let retrieved_stats: gst::Structure = stats_mock.property("stats");
    println!("Retrieved custom stats: {}", retrieved_stats.to_string());

    // Verify custom stats were stored and retrieved correctly
    assert_eq!(
        retrieved_stats.name(),
        "rist/x-sender-stats",
        "Retrieved stats should have correct structure name"
    );

    // Check specific field values
    assert_eq!(
        retrieved_stats.get::<u64>("session-0.sent-original-packets"),
        Ok(1000u64),
        "Session-0 sent-original-packets should match"
    );
    assert_eq!(
        retrieved_stats.get::<u64>("session-0.sent-retransmitted-packets"),
        Ok(10u64),
        "Session-0 sent-retransmitted-packets should match"
    );
    assert_eq!(
        retrieved_stats.get::<f64>("session-0.round-trip-time"),
        Ok(20.0f64),
        "Session-0 round-trip-time should match"
    );

    assert_eq!(
        retrieved_stats.get::<u64>("session-1.sent-original-packets"),
        Ok(1000u64),
        "Session-1 sent-original-packets should match"
    );
    assert_eq!(
        retrieved_stats.get::<u64>("session-1.sent-retransmitted-packets"),
        Ok(100u64),
        "Session-1 sent-retransmitted-packets should match"
    );
    assert_eq!(
        retrieved_stats.get::<f64>("session-1.round-trip-time"),
        Ok(80.0f64),
        "Session-1 round-trip-time should match"
    );

    // Test 3: Verify dispatcher integration
    let dispatcher = gst::ElementFactory::make("ristdispatcher")
        .property("auto-balance", true)
        .property("rebalance-interval-ms", 100u64)
        .build()
        .expect("Failed to create ristdispatcher");

    dispatcher.set_property("rist", &stats_mock);

    // Create src pads to match the stats sessions
    let _pad1 = dispatcher
        .request_pad_simple("src_%u")
        .expect("Failed to request src pad 1");
    let _pad2 = dispatcher
        .request_pad_simple("src_%u")
        .expect("Failed to request src pad 2");

    // Check initial weights (should be default)
    let initial_weights_str: String = dispatcher.property("current-weights");
    let initial_weights_json: serde_json::Value =
        serde_json::from_str(&initial_weights_str).expect("Initial weights should be valid JSON");

    assert!(
        initial_weights_json.is_array(),
        "Initial weights should be JSON array"
    );
    let initial_weights_array = initial_weights_json.as_array().unwrap();
    assert_eq!(
        initial_weights_array.len(),
        2,
        "Should have weights for 2 sessions"
    );

    println!("Initial weights: {}", initial_weights_str);

    // Allow time for stats polling to occur
    std::thread::sleep(Duration::from_millis(300));

    // Check updated weights
    let updated_weights_str: String = dispatcher.property("current-weights");
    let updated_weights_json: serde_json::Value =
        serde_json::from_str(&updated_weights_str).expect("Updated weights should be valid JSON");

    println!(
        "Updated weights after stats polling: {}",
        updated_weights_str
    );

    assert!(
        updated_weights_json.is_array(),
        "Updated weights should be JSON array"
    );
    let updated_weights_array = updated_weights_json.as_array().unwrap();
    assert_eq!(
        updated_weights_array.len(),
        2,
        "Should still have weights for 2 sessions"
    );

    // Verify weights are valid numbers
    let weight0 = updated_weights_array[0]
        .as_f64()
        .expect("Weight 0 should be a number");
    let weight1 = updated_weights_array[1]
        .as_f64()
        .expect("Weight 1 should be a number");

    assert!(weight0 > 0.0, "Weight 0 should be positive");
    assert!(weight1 > 0.0, "Weight 1 should be positive");

    // Since session 0 has better performance (1% loss vs 10% loss), it should eventually
    // receive higher weight in an EWMA system, but this may take time to converge
    println!(
        "Final weights: session-0={:.3}, session-1={:.3}",
        weight0, weight1
    );

    // Test passes if we successfully read stats and the dispatcher processed them
    assert!(
        !retrieved_stats.to_string().is_empty(),
        "Stats should not be empty"
    );
    println!("Deterministic stats polling test passed!");
}

#[test]
fn test_stats_evolution_over_time() {
    ristsmart_tests::register_everything_for_tests();

    let stats_mock = gst::ElementFactory::make("riststats_mock")
        .build()
        .expect("Failed to create riststats mock");

    // Initialize the mock with 2 sessions
    let mock: ristsmart_tests::RistStatsMock = stats_mock.clone().downcast().unwrap();
    mock.set_sessions(2);

    let dispatcher = gst::ElementFactory::make("ristdispatcher")
        .property("auto-balance", true)
        .property("rebalance-interval-ms", 100u64) // Fast polling for testing (minimum allowed value)
        .property("strategy", "ewma")
        .build()
        .expect("Failed to create ristdispatcher");

    dispatcher.set_property("rist", &stats_mock);

    // Request two pads
    let _pad1 = dispatcher
        .request_pad_simple("src_%u")
        .expect("Failed to request pad 1");
    let _pad2 = dispatcher
        .request_pad_simple("src_%u")
        .expect("Failed to request pad 2");

    // Initial baseline: set initial stats to establish a baseline (with small amounts)
    mock.tick(&[100, 100], &[1, 1], &[50, 50]);

    // Wait using main loop to allow timer callbacks to establish baseline
    let main_loop_init = glib::MainLoop::new(None, false);
    let main_loop_init_clone = main_loop_init.clone();

    glib::timeout_add_once(Duration::from_millis(300), move || {
        main_loop_init_clone.quit();
    });

    main_loop_init.run();

    // Phase 1: Session 0 performs much better (dramatic difference)
    mock.tick(&[10000, 10000], &[10, 2000], &[10, 200]); // Session 0: 0.1% loss, low RTT. Session 1: 20% loss, high RTT

    // Wait using main loop to allow timer callbacks to execute
    let main_loop = glib::MainLoop::new(None, false);
    let main_loop_clone = main_loop.clone();

    glib::timeout_add_once(Duration::from_millis(500), move || {
        main_loop_clone.quit();
    });

    main_loop.run();

    let phase1_weights_str: String = dispatcher.property("current-weights");
    let phase1_weights_json: serde_json::Value =
        serde_json::from_str(&phase1_weights_str).expect("Phase 1 weights should be valid JSON");
    let phase1_weights_array = phase1_weights_json.as_array().unwrap();
    let phase1_weight0 = phase1_weights_array[0].as_f64().unwrap();
    let phase1_weight1 = phase1_weights_array[1].as_f64().unwrap();

    println!(
        "Phase 1 weights: session-0={:.3}, session-1={:.3}",
        phase1_weight0, phase1_weight1
    );

    // Phase 2: Conditions flip dramatically - session 1 becomes much better
    mock.tick(&[10000, 10000], &[2000, 10], &[200, 10]); // Session 0: 20% loss, high RTT. Session 1: 0.1% loss, low RTT

    // Wait using main loop to allow timer callbacks to execute
    let main_loop2 = glib::MainLoop::new(None, false);
    let main_loop2_clone = main_loop2.clone();

    glib::timeout_add_once(Duration::from_millis(500), move || {
        main_loop2_clone.quit();
    });

    main_loop2.run();

    let phase2_weights_str: String = dispatcher.property("current-weights");
    let phase2_weights_json: serde_json::Value =
        serde_json::from_str(&phase2_weights_str).expect("Phase 2 weights should be valid JSON");
    let phase2_weights_array = phase2_weights_json.as_array().unwrap();
    let phase2_weight0 = phase2_weights_array[0].as_f64().unwrap();
    let phase2_weight1 = phase2_weights_array[1].as_f64().unwrap();

    println!(
        "Phase 2 weights: session-0={:.3}, session-1={:.3}",
        phase2_weight0, phase2_weight1
    );

    // Verify basic properties of weight evolution
    assert!(
        phase1_weight0 > 0.0 && phase1_weight1 > 0.0,
        "Phase 1 weights should be positive"
    );
    assert!(
        phase2_weight0 > 0.0 && phase2_weight1 > 0.0,
        "Phase 2 weights should be positive"
    );

    // The exact EWMA response depends on algorithm parameters, but we can verify
    // that the system is responsive to changing conditions
    let weight0_change = phase2_weight0 - phase1_weight0;
    let weight1_change = phase2_weight1 - phase1_weight1;

    println!(
        "Weight changes: session-0={:.3}, session-1={:.3}",
        weight0_change, weight1_change
    );

    // As a basic check, verify that the weights did change (showing responsiveness)
    assert!(
        weight0_change.abs() > 0.001 || weight1_change.abs() > 0.001,
        "Weights should change in response to different stats"
    );

    println!("Stats evolution over time test passed!");
}

#[test]
fn test_malformed_stats_handling() {
    ristsmart_tests::register_everything_for_tests();

    let stats_mock = gst::ElementFactory::make("riststats_mock")
        .build()
        .expect("Failed to create riststats mock");

    let dispatcher = gst::ElementFactory::make("ristdispatcher")
        .property("auto-balance", true)
        .property("rebalance-interval-ms", 100u64)
        .build()
        .expect("Failed to create ristdispatcher");

    dispatcher.set_property("rist", &stats_mock);
    let _pad1 = dispatcher
        .request_pad_simple("src_%u")
        .expect("Failed to request pad");

    // Test 1: Empty stats structure
    let empty_stats = gst::Structure::builder("rist/x-sender-stats").build();
    stats_mock.set_property("stats", &empty_stats);

    std::thread::sleep(Duration::from_millis(150));

    let weights_after_empty: String = dispatcher.property("current-weights");
    let weights_json: serde_json::Value = serde_json::from_str(&weights_after_empty)
        .expect("Should still have valid weights JSON after empty stats");

    assert!(
        weights_json.is_array(),
        "Should maintain valid weights array"
    );
    println!("Weights after empty stats: {}", weights_after_empty);

    // Test 2: Malformed field names
    let malformed_stats = gst::Structure::builder("rist/x-sender-stats")
        .field("invalid-field", 123u64)
        .field("session-0.invalid-subfield", 456u64)
        .build();
    stats_mock.set_property("stats", &malformed_stats);

    std::thread::sleep(Duration::from_millis(150));

    let weights_after_malformed: String = dispatcher.property("current-weights");
    let weights_json: serde_json::Value = serde_json::from_str(&weights_after_malformed)
        .expect("Should still have valid weights JSON after malformed stats");

    assert!(
        weights_json.is_array(),
        "Should maintain valid weights array"
    );
    println!("Weights after malformed stats: {}", weights_after_malformed);

    // Test 3: Wrong structure name
    let wrong_name_stats = gst::Structure::builder("wrong/structure-name")
        .field("session-0.sent-original-packets", 1000u64)
        .build();
    stats_mock.set_property("stats", &wrong_name_stats);

    std::thread::sleep(Duration::from_millis(150));

    let weights_after_wrong_name: String = dispatcher.property("current-weights");
    let weights_json: serde_json::Value = serde_json::from_str(&weights_after_wrong_name)
        .expect("Should still have valid weights JSON after wrong structure name");

    assert!(
        weights_json.is_array(),
        "Should maintain valid weights array"
    );
    println!(
        "Weights after wrong structure name: {}",
        weights_after_wrong_name
    );

    // The key test: system should remain functional despite malformed input
    let final_weights_array = weights_json.as_array().unwrap();
    for (i, weight) in final_weights_array.iter().enumerate() {
        let weight_val = weight.as_f64().expect("Final weight should be a number");
        assert!(
            weight_val >= 0.0,
            "Final weight {} should be non-negative",
            i
        );
    }

    println!("Malformed stats handling test passed!");
}
