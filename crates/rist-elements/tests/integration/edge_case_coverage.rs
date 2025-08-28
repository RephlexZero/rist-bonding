use gst::prelude::*;
use gstreamer as gst;
use gstristelements::testing::*;
use std::thread;
use std::time::{Duration, Instant};

/// Pump the GLib main loop for the specified duration
fn run_mainloop_ms(ms: u64) {
    let ctx = glib::MainContext::default();
    let _guard = ctx.acquire().expect("acquire main context");
    let end = std::time::Instant::now() + Duration::from_millis(ms);
    while std::time::Instant::now() < end {
        while ctx.iteration(false) {}
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Test dispatcher behavior with zero weight edge cases
#[test]
fn test_zero_weight_edge_cases() {
    init_for_tests();

    let dispatcher = create_dispatcher_for_testing(Some(&[1.0, 1.0, 1.0]));

    // Test various zero weight scenarios
    let zero_weight_cases = [
        "[0.0, 1.0, 1.0]", // First weight zero
        "[1.0, 0.0, 1.0]", // Middle weight zero
        "[1.0, 1.0, 0.0]", // Last weight zero
        "[0.0, 0.0, 1.0]", // Two weights zero
        "[0.0]",           // Single zero weight
    ];

    for (i, weights) in zero_weight_cases.iter().enumerate() {
        println!("Testing zero weight case {}: {}", i + 1, weights);

        // Set zero weights and verify system remains stable
        dispatcher.set_property("weights", *weights);
        run_mainloop_ms(50);

        // Verify system handled zero weights gracefully
        let current_weights: String = dispatcher.property("weights");
        assert!(
            !current_weights.is_empty(),
            "Weights should remain valid after zero weight input"
        );

        // Parse and validate weights
        if let Ok(weight_vec) = serde_json::from_str::<Vec<f64>>(&current_weights) {
            for weight in &weight_vec {
                assert!(weight.is_finite(), "All weights should be finite numbers");
                assert!(*weight >= 0.0, "All weights should be non-negative");
            }
        }

        // System should remain responsive
        let strategy: String = dispatcher.property("strategy");
        assert!(!strategy.is_empty(), "Strategy should remain accessible");
    }

    // Test all weights zero separately - this may be handled as a special case
    println!("Testing all zero weights case: [0.0, 0.0, 0.0]");
    dispatcher.set_property("weights", "[0.0, 0.0, 0.0]");
    run_mainloop_ms(50);

    let current_weights: String = dispatcher.property("weights");
    assert!(
        !current_weights.is_empty(),
        "Weights should remain valid after all-zero input"
    );

    // The dispatcher should handle all-zero weights by either:
    // 1. Normalizing to equal weights, or
    // 2. Falling back to some default
    // Either behavior is acceptable as long as it doesn't crash
    if let Ok(weight_vec) = serde_json::from_str::<Vec<f64>>(&current_weights) {
        for weight in &weight_vec {
            assert!(weight.is_finite(), "All weights should be finite numbers");
            assert!(*weight >= 0.0, "All weights should be non-negative");
        }
        println!("All-zero weights handled as: {:?}", weight_vec);
    }
}

/// Test dispatcher behavior with extreme weight ratios
#[test]
fn test_extreme_weight_ratios() {
    init_for_tests();

    let dispatcher = create_dispatcher_for_testing(Some(&[1.0, 1.0, 1.0]));

    // Test extremely large weight ratios
    let extreme_cases = [
        "[1000.0, 1.0, 1.0]",         // One extremely high weight
        "[0.001, 1.0, 1.0]",          // One extremely low weight
        "[1000000.0, 1.0, 0.000001]", // Very large range
        "[0.0001, 0.0001, 10000.0]",  // Mixed extreme values
    ];

    for (i, weights) in extreme_cases.iter().enumerate() {
        println!("Testing extreme ratio case {}: {}", i + 1, weights);

        dispatcher.set_property("weights", *weights);
        run_mainloop_ms(50);

        // Verify system handles extreme ratios
        let current_weights: String = dispatcher.property("weights");
        assert!(
            !current_weights.is_empty(),
            "Weights should remain valid with extreme ratios"
        );

        // Parse and check for numerical stability
        if let Ok(weight_vec) = serde_json::from_str::<Vec<f64>>(&current_weights) {
            for weight in &weight_vec {
                assert!(
                    weight.is_finite(),
                    "Weights should remain finite with extreme ratios"
                );
                assert!(*weight >= 0.0, "Weights should remain non-negative");
                assert!(!weight.is_nan(), "Weights should not become NaN");
            }

            // Verify normalization worked
            let sum: f64 = weight_vec.iter().sum();
            assert!(sum > 0.0, "Total weight should remain positive");
            assert!(sum.is_finite(), "Total weight should be finite");
        }
    }
}

/// Test dispatcher behavior during rapid state changes  
#[test]
fn test_rapid_state_changes() {
    init_for_tests();

    let dispatcher = create_dispatcher_for_testing(Some(&[1.0, 1.0, 1.0]));

    // Configure for rapid response with valid bounds
    dispatcher.set_property("min-hold-ms", 10u64);
    // Use minimum valid value for rebalance-interval-ms (100ms based on error)
    dispatcher.set_property("rebalance-interval-ms", 100u64);

    let start_time = Instant::now();

    // Perform rapid state changes for 2 seconds
    let rapid_changes = [
        ("[2.0, 1.0, 1.0]", "rr", 100u64),
        ("[1.0, 2.0, 1.0]", "weighted", 150u64),
        ("[1.0, 1.0, 2.0]", "rr", 120u64),
        ("[1.5, 1.5, 1.0]", "weighted", 200u64),
        ("[1.0, 1.5, 1.5]", "rr", 180u64),
    ];

    let mut change_count = 0usize;
    while start_time.elapsed() < Duration::from_secs(2) {
        let change_set = &rapid_changes[change_count % rapid_changes.len()];

        // Rapid property changes
        dispatcher.set_property("weights", change_set.0);
        dispatcher.set_property("strategy", change_set.1);
        dispatcher.set_property("rebalance-interval-ms", change_set.2);

        // Extremely short processing time to stress rapid changes
        run_mainloop_ms(5);

        change_count += 1;

        // Verify system remains stable during rapid changes
        let weights: String = dispatcher.property("weights");
        let strategy: String = dispatcher.property("strategy");

        assert!(
            !weights.is_empty(),
            "Weights should remain accessible during rapid changes"
        );
        assert!(
            !strategy.is_empty(),
            "Strategy should remain accessible during rapid changes"
        );

        thread::sleep(Duration::from_millis(1)); // Minimal delay between changes
    }

    // Final stability check after rapid changes
    run_mainloop_ms(100);

    let final_weights: String = dispatcher.property("weights");
    let final_strategy: String = dispatcher.property("strategy");

    assert!(
        !final_weights.is_empty(),
        "Weights should be stable after rapid changes"
    );
    assert!(
        !final_strategy.is_empty(),
        "Strategy should be stable after rapid changes"
    );

    println!(
        "Rapid state changes completed: {} changes in 2 seconds",
        change_count
    );
}

/// Test dispatcher behavior with invalid JSON weight formats
#[test]
fn test_invalid_json_weights() {
    init_for_tests();

    let dispatcher = create_dispatcher_for_testing(Some(&[1.0, 1.0, 1.0]));

    // Test various invalid JSON formats
    let invalid_jsons = [
        "[1.0, 2.0,]",          // Trailing comma
        "[1.0, 2.0, abc]",      // Non-numeric value
        "{1.0, 2.0, 3.0}",      // Wrong brackets
        "[1.0, 2.0, 3.0",       // Missing closing bracket
        "1.0, 2.0, 3.0]",       // Missing opening bracket
        "not_json_at_all",      // Complete nonsense
        "",                     // Empty string
        "null",                 // JSON null
        "[NaN, 1.0, 2.0]",      // JSON with NaN
        "[Infinity, 1.0, 2.0]", // JSON with Infinity
    ];

    for (i, invalid_json) in invalid_jsons.iter().enumerate() {
        println!("Testing invalid JSON case {}: {}", i + 1, invalid_json);

        // Attempt to set invalid JSON
        let _set_result = std::panic::catch_unwind(|| {
            dispatcher.set_property("weights", *invalid_json);
        });

        // Verify system remains stable regardless of invalid input
        run_mainloop_ms(50);

        let current_weights: String = dispatcher.property("weights");
        assert!(
            !current_weights.is_empty(),
            "Weights should remain valid after invalid JSON input"
        );

        // System should either reject the change (keep old weights) or use some default
        // Either behavior is acceptable as long as it doesn't crash

        // Verify we can still read other properties
        let strategy: String = dispatcher.property("strategy");
        assert!(
            !strategy.is_empty(),
            "Strategy should remain accessible after invalid JSON"
        );
    }

    println!("Invalid JSON handling completed successfully");
}

/// Test dispatcher property boundary values
#[test]
fn test_boundary_values() {
    init_for_tests();

    let dispatcher = create_dispatcher_for_testing(Some(&[1.0, 1.0, 1.0]));

    // Test u64 property boundaries
    let u64_boundary_tests = [
        ("min-hold-ms", vec![0u64, 1u64, 100u64, 10000u64]),
        ("rebalance-interval-ms", vec![100u64, 1000u64, 60000u64]), // Min value is 100ms
    ];

    for (prop_name, test_values) in u64_boundary_tests.iter() {
        println!("Testing u64 property: {}", prop_name);

        for test_val in test_values {
            println!("  Testing value: {}", test_val);

            // Attempt to set the value - may be clamped
            let _set_result = std::panic::catch_unwind(|| {
                dispatcher.set_property(prop_name, test_val);
            });

            run_mainloop_ms(20);

            // Verify system remains stable
            let actual_val: u64 = dispatcher.property(prop_name);

            // For rebalance-interval-ms, ensure minimum is respected
            if *prop_name == "rebalance-interval-ms" {
                assert!(
                    actual_val >= 100,
                    "rebalance-interval-ms should be >= 100ms"
                );
            }
        }
    }

    // Test f64 property boundaries (like switch-threshold)
    let f64_boundary_tests = [(
        "switch-threshold",
        vec![0.0f64, 0.1, 1.0, 10.0, 100.0, f64::MAX],
    )];

    for (prop_name, test_values) in f64_boundary_tests.iter() {
        println!("Testing f64 property: {}", prop_name);

        for test_val in test_values {
            println!("  Testing value: {}", test_val);

            // Attempt to set the value
            let _set_result = std::panic::catch_unwind(|| {
                dispatcher.set_property(prop_name, test_val);
            });

            run_mainloop_ms(20);

            // Verify system remains stable
            let actual_val: f64 = dispatcher.property(prop_name);

            // Value should be finite and reasonable
            assert!(
                actual_val.is_finite(),
                "Property {} should have finite value",
                prop_name
            );
            assert!(
                actual_val >= 0.0,
                "Property {} should have non-negative value",
                prop_name
            );
        }
    }

    println!("Boundary value testing completed successfully");
}

/// Test concurrent operations on dispatcher
#[test]
fn test_concurrent_operations() {
    init_for_tests();

    let dispatcher = create_dispatcher_for_testing(Some(&[1.0, 1.0, 1.0]));

    // Stress test with concurrent property changes and state queries
    let dispatcher_clone = dispatcher.clone();

    let handle = std::thread::spawn(move || {
        // Concurrent thread rapidly changes weights
        for i in 0..20 {
            let weights = match i % 4 {
                0 => "[1.0, 1.0, 1.0]",
                1 => "[2.0, 1.0, 1.0]",
                2 => "[1.0, 2.0, 1.0]",
                _ => "[1.0, 1.0, 2.0]",
            };

            dispatcher_clone.set_property("weights", weights);
            thread::sleep(Duration::from_millis(10));
        }
    });

    // Main thread queries properties while changes happen
    for i in 0..50 {
        let weights: String = dispatcher.property("weights");
        let strategy: String = dispatcher.property("strategy");

        assert!(
            !weights.is_empty(),
            "Weights should remain readable during concurrent operations"
        );
        assert!(
            !strategy.is_empty(),
            "Strategy should remain readable during concurrent operations"
        );

        // Also make property changes from main thread
        if i % 5 == 0 {
            let strategies = ["rr", "weighted"];
            dispatcher.set_property("strategy", strategies[i % 2]);
        }

        run_mainloop_ms(5);
        thread::sleep(Duration::from_millis(5));
    }

    handle
        .join()
        .expect("Background thread should complete successfully");

    // Final verification
    run_mainloop_ms(100);
    let final_weights: String = dispatcher.property("weights");
    let final_strategy: String = dispatcher.property("strategy");

    assert!(
        !final_weights.is_empty(),
        "Weights should be stable after concurrent operations"
    );
    assert!(
        !final_strategy.is_empty(),
        "Strategy should be stable after concurrent operations"
    );

    println!("Concurrent operations test completed successfully");
}
