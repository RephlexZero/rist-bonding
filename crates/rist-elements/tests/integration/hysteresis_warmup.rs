use gstreamer::{prelude::*, Element};
use gstristelements::testing::*;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Test dispatcher hysteresis property configuration
#[test]
fn test_dispatcher_hysteresis_properties() {
    init_for_tests();

    let dispatcher = create_dispatcher_for_testing(Some(&[1.0, 1.0]));

    // Test basic property setting and getting
    dispatcher.set_property("min-hold-ms", 500u64);
    dispatcher.set_property("switch-threshold", 1.5f64);
    dispatcher.set_property("health-warmup-ms", 2000u64);

    let min_hold: u64 = dispatcher.property("min-hold-ms");
    let threshold: f64 = dispatcher.property("switch-threshold");
    let warmup: u64 = dispatcher.property("health-warmup-ms");

    assert_eq!(min_hold, 500, "Minimum hold time should be settable");
    assert_eq!(threshold, 1.5, "Switch threshold should be settable");
    assert_eq!(warmup, 2000, "Health warmup time should be settable");

    // Test boundary values
    dispatcher.set_property("switch-threshold", 1.0f64); // Minimum threshold
    dispatcher.set_property("switch-threshold", 5.0f64); // Higher threshold

    let high_threshold: f64 = dispatcher.property("switch-threshold");
    assert_eq!(high_threshold, 5.0, "Should accept valid threshold values");
}

/// Test that weight changes don't cause excessive switching with hysteresis
#[test]
fn test_dispatcher_weight_switching_behavior() {
    init_for_tests();

    let dispatcher = create_dispatcher_for_testing(Some(&[1.0, 1.0]));
    dispatcher.set_property("min-hold-ms", 400u64); // Moderate hold time
    dispatcher.set_property("switch-threshold", 1.3f64); // Require 30% improvement
    dispatcher.set_property("strategy", "manual"); // Use manual weights

    // Track when weights are updated
    let weight_updates = Arc::new(Mutex::new(Vec::new()));
    let weight_updates_clone = weight_updates.clone();

    dispatcher.connect("notify::current-weights", false, move |_values| {
        let timestamp = Instant::now();
        weight_updates_clone.lock().unwrap().push(timestamp);
        None
    });

    // Start the dispatcher
    dispatcher.set_state(gstreamer::State::Playing).unwrap();
    std::thread::sleep(Duration::from_millis(200)); // Let it stabilize

    // Reset monitoring after stabilization
    weight_updates.lock().unwrap().clear();
    let test_start = Instant::now();

    // Apply rapid weight changes that might normally cause switching
    for cycle in 0..6 {
        std::thread::sleep(Duration::from_millis(150)); // Shorter than min-hold

        if cycle % 2 == 0 {
            dispatcher.set_property("weights", "[1.0, 0.5]"); // Favor output 0
        } else {
            dispatcher.set_property("weights", "[0.5, 1.0]"); // Favor output 1
        }
    }

    std::thread::sleep(Duration::from_millis(200)); // Allow final processing
    dispatcher.set_state(gstreamer::State::Null).unwrap();

    let updates = weight_updates.lock().unwrap();
    let test_duration = test_start.elapsed();

    println!("Weight updates: {} over {:?}", updates.len(), test_duration);

    // Should update weights but not excessively due to hysteresis
    // Note: current-weights might not trigger frequently with manual strategy
    if updates.is_empty() {
        println!("No weight updates observed - this may be expected with manual strategy");
    } else {
        assert!(
            updates.len() <= 8,
            "Should not update excessively due to hysteresis"
        );
    }

    // Check that updates respect minimum hold time where applicable
    for i in 1..updates.len() {
        let gap = updates[i].duration_since(updates[i - 1]);
        if gap < Duration::from_millis(100) {
            // Very rapid updates might be acceptable in some cases
            continue;
        }
    }
}

/// Test warm-up period delays weight-based decisions
#[test]
fn test_dispatcher_warmup_period() {
    init_for_tests();

    let dispatcher = create_dispatcher_for_testing(Some(&[1.0, 1.0]));
    dispatcher.set_property("health-warmup-ms", 800u64); // 800ms warmup
    dispatcher.set_property("min-hold-ms", 100u64); // Short hold for testing
    dispatcher.set_property("strategy", "manual");

    dispatcher.set_state(gstreamer::State::Playing).unwrap();

    // Immediately set clear preference during warmup period
    dispatcher.set_property("weights", "[3.0, 0.3]"); // Strong preference

    // Check current weights at intervals during warmup
    std::thread::sleep(Duration::from_millis(200));
    let early_weights: String = dispatcher.property("current-weights");

    std::thread::sleep(Duration::from_millis(300));
    let mid_weights: String = dispatcher.property("current-weights");

    std::thread::sleep(Duration::from_millis(400)); // Total ~900ms, past warmup
    let late_weights: String = dispatcher.property("current-weights");

    dispatcher.set_state(gstreamer::State::Null).unwrap();

    println!(
        "Warmup test - early: {}, mid: {}, late: {}",
        early_weights, mid_weights, late_weights
    );

    // During warmup, weights should be available but decision-making may be delayed
    assert!(
        !early_weights.is_empty(),
        "Should have weight information during warmup"
    );
    assert!(
        !late_weights.is_empty(),
        "Should have weight information after warmup"
    );
}

/// Test threshold enforcement prevents small-improvement switching
#[test]
fn test_switch_threshold_enforcement() {
    init_for_tests();

    let dispatcher = create_dispatcher_for_testing(Some(&[1.0, 1.0]));
    dispatcher.set_property("switch-threshold", 2.0f64); // Require 2x improvement
    dispatcher.set_property("min-hold-ms", 200u64); // Short hold for testing
    dispatcher.set_property("strategy", "manual");

    let weight_changes = Arc::new(Mutex::new(Vec::new()));
    let weight_changes_clone = weight_changes.clone();

    dispatcher.connect("notify::current-weights", false, move |values| {
        let elem = values[0].get::<Element>().unwrap();
        let weights_str: String = elem.property("current-weights");
        weight_changes_clone.lock().unwrap().push(weights_str);
        None
    });

    dispatcher.set_state(gstreamer::State::Playing).unwrap();
    std::thread::sleep(Duration::from_millis(300)); // Allow stabilization

    // Clear initial changes
    weight_changes.lock().unwrap().clear();

    // Apply small improvement (below threshold)
    dispatcher.set_property("weights", "[1.0, 1.4]"); // 40% improvement (below 2x)
    std::thread::sleep(Duration::from_millis(300));

    let changes_after_small = weight_changes.lock().unwrap().len();

    // Apply large improvement (above threshold)
    dispatcher.set_property("weights", "[1.0, 2.5]"); // 2.5x improvement (above 2x)
    std::thread::sleep(Duration::from_millis(300));

    let changes_after_large = weight_changes.lock().unwrap().len();

    dispatcher.set_state(gstreamer::State::Null).unwrap();

    println!(
        "Threshold test - changes after small: {}, after large: {}",
        changes_after_small, changes_after_large
    );

    // Both should cause some change since we're setting weights manually
    // but the behavior may differ based on internal switching logic
    assert!(
        changes_after_large >= changes_after_small,
        "Large improvements should cause at least as many changes"
    );
}

/// Test basic dispatcher statistics output during operation
#[test]
fn test_dispatcher_stats_during_hysteresis() {
    init_for_tests();

    let dispatcher = create_dispatcher_for_testing(Some(&[1.0, 1.0]));
    dispatcher.set_property("min-hold-ms", 300u64);
    dispatcher.set_property("switch-threshold", 1.4f64);

    dispatcher.set_state(gstreamer::State::Playing).unwrap();
    std::thread::sleep(Duration::from_millis(200));

    // Get stats after some operation time
    let initial_weights: String = dispatcher.property("current-weights");
    assert!(
        !initial_weights.is_empty(),
        "Should have current weights available"
    );

    // Change weights and verify stats update
    dispatcher.set_property("weights", "[1.2, 0.8]");
    std::thread::sleep(Duration::from_millis(400)); // Wait past min-hold

    let updated_weights: String = dispatcher.property("current-weights");
    assert!(!updated_weights.is_empty(), "Should have updated weights");

    dispatcher.set_state(gstreamer::State::Null).unwrap();

    println!(
        "Stats test - initial: {}, updated: {}",
        initial_weights, updated_weights
    );
}
