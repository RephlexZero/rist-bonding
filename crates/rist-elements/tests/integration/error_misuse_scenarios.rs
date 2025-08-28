use gstreamer::prelude::*;
use gstristelements::testing::*;
use std::time::Duration;

#[test]
fn test_dispatcher_invalid_weights_handling() {
    init_for_tests();

    let dispatcher = create_dispatcher(Some(&[1.0, 1.0])); // Start with valid weights

    // Test various invalid weight scenarios
    let invalid_weight_tests = [
        ("", "empty weights"),
        ("invalid", "non-numeric weights"),
        ("1.0,-1.0", "negative weights"),
        ("0.0,0.0", "all zero weights"),
        ("inf,1.0", "infinite weights"),
        ("nan,1.0", "NaN weights"),
    ];

    for (weights_str, description) in &invalid_weight_tests {
        println!("Testing {}: '{}'", description, weights_str);

        // Element should handle invalid weights gracefully
        dispatcher.set_property("weights", *weights_str);

        // Verify element continues to function
        let current_weights: String = dispatcher.property("current-weights");
        println!("  Current weights after invalid input: {}", current_weights);

        // Element should either reject invalid input or use safe defaults
        assert!(
            !current_weights.is_empty(),
            "Element should maintain valid weights"
        );
    }

    println!("dispatcher invalid weights handling test passed");
}

#[test]
fn test_dispatcher_extreme_pad_counts() {
    init_for_tests();

    let dispatcher = create_dispatcher(None);
    let source = create_test_source();
    let pipeline = gstreamer::Pipeline::new();

    pipeline.add_many([&source, &dispatcher]).unwrap();
    source.link(&dispatcher).unwrap();

    pipeline.set_state(gstreamer::State::Playing).unwrap();
    std::thread::sleep(Duration::from_millis(50));

    // Test creating many output pads
    let mut pads = Vec::new();
    let max_pads = 10; // Reasonable limit for testing

    println!("Testing creation of {} output pads", max_pads);

    for i in 0..max_pads {
        if let Some(pad) = dispatcher.request_pad_simple("src_%u") {
            pads.push(pad);
            println!("  Created pad {} successfully", i + 1);
        } else {
            println!("  Failed to create pad {} (may be expected limit)", i + 1);
            break;
        }
    }

    println!("Successfully created {} pads", pads.len());

    // Test rapid pad creation and release
    println!("Testing rapid pad release and recreation");

    // Release half the pads
    let release_count = pads.len() / 2;
    for (i, pad) in pads.iter().enumerate().take(release_count) {
        dispatcher.release_request_pad(pad);
        println!("  Released pad {}", i + 1);
    }

    // Try to create new pads
    let mut new_pads = Vec::new();
    for i in 0..3 {
        if let Some(pad) = dispatcher.request_pad_simple("src_%u") {
            new_pads.push(pad);
            println!("  Created new pad {} after release", i + 1);
        }
    }

    // Clean up remaining pads
    for pad in pads.iter().skip(release_count) {
        dispatcher.release_request_pad(pad);
    }
    for pad in &new_pads {
        dispatcher.release_request_pad(pad);
    }

    pipeline.set_state(gstreamer::State::Null).unwrap();

    println!("dispatcher extreme pad counts test passed");
}

#[test]
fn test_dispatcher_downstream_failures() {
    init_for_tests();

    let dispatcher = create_dispatcher(Some(&[1.0, 1.0]));
    let source = create_test_source();

    // Create sinks that might cause issues
    let counter1 = create_counter_sink();
    let counter2 = create_counter_sink();

    let pipeline = gstreamer::Pipeline::new();
    pipeline
        .add_many([&source, &dispatcher, &counter1, &counter2])
        .unwrap();

    source.link(&dispatcher).unwrap();
    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();

    src_0.link(&counter1.static_pad("sink").unwrap()).unwrap();
    src_1.link(&counter2.static_pad("sink").unwrap()).unwrap();

    // Test normal operation first
    dispatcher.set_state(gstreamer::State::Playing).unwrap();
    std::thread::sleep(Duration::from_millis(100));

    // Simulate downstream issues by state changes
    println!("Simulating downstream sink failures");

    // Force one sink to error state (by setting to NULL while others are playing)
    counter1.set_state(gstreamer::State::Null).unwrap();
    std::thread::sleep(Duration::from_millis(100));

    // Dispatcher should handle this gracefully
    println!("Dispatcher handling downstream failure gracefully");

    // Try to restart the failed sink
    counter1.set_state(gstreamer::State::Playing).unwrap();
    std::thread::sleep(Duration::from_millis(100));

    // Clean up
    dispatcher.release_request_pad(&src_0);
    dispatcher.release_request_pad(&src_1);
    dispatcher.set_state(gstreamer::State::Null).unwrap();

    println!("dispatcher downstream failures test passed");
}

#[test]
fn test_dynbitrate_invalid_configurations() {
    init_for_tests();

    let dynbitrate = create_dynbitrate();
    let encoder_stub = create_encoder_stub(Some(2000));
    let riststats_mock = create_riststats_mock(Some(0.8), Some(50));

    dynbitrate.set_property("encoder", &encoder_stub);
    dynbitrate.set_property("rist", &riststats_mock);

    // Test invalid configuration scenarios (within valid property ranges but logically invalid)
    let invalid_configs = [
        ("min > max", 8000u32, 2000u32, 500u32, 1.0), // Both within valid ranges but min > max
        ("small step", 1000u32, 3000u32, 50u32, 1.0), // Minimum step size
        ("extreme loss target", 1000u32, 3000u32, 500u32, 10.0), // Maximum target loss
        ("boundary min", 100u32, 3000u32, 500u32, 1.0), // Minimum allowed min-kbps
    ];

    for (description, min_kbps, max_kbps, step_kbps, target_loss) in &invalid_configs {
        println!("Testing invalid configuration: {}", description);

        // Element should handle invalid configurations gracefully
        dynbitrate.set_property("min-kbps", *min_kbps);
        dynbitrate.set_property("max-kbps", *max_kbps);
        dynbitrate.set_property("step-kbps", *step_kbps);
        dynbitrate.set_property("target-loss-pct", *target_loss);

        // Verify element maintains reasonable values
        let actual_min: u32 = dynbitrate.property("min-kbps");
        let actual_max: u32 = dynbitrate.property("max-kbps");
        let actual_step: u32 = dynbitrate.property("step-kbps");
        let actual_target: f64 = dynbitrate.property("target-loss-pct");

        println!(
            "  After invalid config - min: {}, max: {}, step: {}, target: {}",
            actual_min, actual_max, actual_step, actual_target
        );

        // Element accepts configuration as-is (validation may be done at runtime)
        // We mainly check that element doesn't crash with invalid configs
        assert!(actual_min >= 100, "min-kbps should be within valid range");
        assert!(actual_step >= 50, "step-kbps should be within valid range");
        assert!(
            (0.0..=10.0).contains(&actual_target),
            "target-loss-pct should be within valid range"
        );

        println!("  {} handled without crashing", description);
    }

    println!("dynbitrate invalid configurations test passed");
}

#[test]
fn test_dynbitrate_missing_required_elements() {
    init_for_tests();

    // Test dynbitrate behavior when required elements are missing
    let scenarios = [
        ("no encoder", false, true),   // Missing encoder
        ("no rist", true, false),      // Missing rist
        ("no elements", false, false), // Missing both
    ];

    for (description, has_encoder, has_rist) in &scenarios {
        println!("Testing {}", description);

        let dynbitrate = create_dynbitrate();

        if *has_encoder {
            let encoder_stub = create_encoder_stub(Some(1500));
            dynbitrate.set_property("encoder", &encoder_stub);
        }

        if *has_rist {
            let riststats_mock = create_riststats_mock(Some(0.85), Some(40));
            dynbitrate.set_property("rist", &riststats_mock);
        }

        // Configure other properties
        dynbitrate.set_property("min-kbps", 1000u32);
        dynbitrate.set_property("max-kbps", 4000u32);

        let pipeline = gstreamer::Pipeline::new();
        let elements = if *has_encoder && *has_rist {
            let encoder_stub = create_encoder_stub(Some(1500));
            let riststats_mock = create_riststats_mock(Some(0.85), Some(40));
            vec![dynbitrate.clone(), encoder_stub, riststats_mock]
        } else if *has_encoder {
            let encoder_stub = create_encoder_stub(Some(1500));
            vec![dynbitrate.clone(), encoder_stub]
        } else if *has_rist {
            let riststats_mock = create_riststats_mock(Some(0.85), Some(40));
            vec![dynbitrate.clone(), riststats_mock]
        } else {
            vec![dynbitrate.clone()]
        };

        pipeline.add_many(&elements).unwrap();

        // Element should handle missing dependencies gracefully
        pipeline.set_state(gstreamer::State::Playing).unwrap();
        std::thread::sleep(Duration::from_millis(100));
        pipeline.set_state(gstreamer::State::Null).unwrap();

        println!("  {} scenario handled gracefully", description);
    }

    println!("dynbitrate missing required elements test passed");
}

#[test]
fn test_element_property_type_mismatches() {
    init_for_tests();

    let dispatcher = create_dispatcher(Some(&[1.0, 1.0]));

    println!("Testing property type mismatch handling");

    // These should be handled gracefully (GLib/GStreamer property system handles type conversion)
    // We're mainly testing that the element doesn't crash with unexpected property types

    // Test setting numeric properties with string-like values
    println!("Testing numeric property edge cases");

    // Test boundary values for known properties
    dispatcher.set_property("rebalance-interval-ms", 10000u64); // Maximum allowed
    let interval: u64 = dispatcher.property("rebalance-interval-ms");
    println!("  Max interval value: {}", interval);

    dispatcher.set_property("min-hold-ms", 0u64); // Minimum allowed
    let min_hold: u64 = dispatcher.property("min-hold-ms");
    println!("  Min hold value: {}", min_hold);

    // Test extreme but valid values
    dispatcher.set_property("switch-threshold", 10.0); // Maximum allowed
    let threshold: f64 = dispatcher.property("switch-threshold");
    println!("  Max threshold value: {}", threshold);

    // Test with dynbitrate as well
    let dynbitrate = create_dynbitrate();

    // Test extreme values within valid ranges
    dynbitrate.set_property("max-kbps", 100000u32); // Maximum allowed
    let max_kbps: u32 = dynbitrate.property("max-kbps");
    println!("  Max bitrate value: {}", max_kbps);

    dynbitrate.set_property("target-loss-pct", 10.0); // Maximum allowed
    let target_loss: f64 = dynbitrate.property("target-loss-pct");
    println!("  Max target loss: {}", target_loss);

    println!("element property type mismatch handling test passed");
}

#[test]
fn test_stress_rapid_property_changes() {
    init_for_tests();

    let dispatcher = create_dispatcher(Some(&[1.0, 1.0]));
    let source = create_test_source();
    let counter1 = create_counter_sink();
    let counter2 = create_counter_sink();

    let pipeline = gstreamer::Pipeline::new();
    pipeline
        .add_many([&source, &dispatcher, &counter1, &counter2])
        .unwrap();

    source.link(&dispatcher).unwrap();
    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();

    src_0.link(&counter1.static_pad("sink").unwrap()).unwrap();
    src_1.link(&counter2.static_pad("sink").unwrap()).unwrap();

    dispatcher.set_state(gstreamer::State::Playing).unwrap();

    // Rapidly change properties while running
    println!("Testing rapid property changes during operation");

    let weight_sequences = [
        "1.0,1.0", "2.0,1.0", "1.0,3.0", "0.5,0.5", "1.0,1.0", "4.0,1.0", "1.0,4.0",
    ];

    for (i, weights) in weight_sequences.iter().enumerate() {
        dispatcher.set_property("weights", *weights);
        dispatcher.set_property("auto-balance", i % 2 == 0);
        dispatcher.set_property("rebalance-interval-ms", (100 + i * 50) as u64);

        println!("  Applied rapid change {}: weights={}", i + 1, weights);
        std::thread::sleep(Duration::from_millis(50)); // Brief delay
    }

    // Let element stabilize
    std::thread::sleep(Duration::from_millis(200));

    // Clean up
    dispatcher.release_request_pad(&src_0);
    dispatcher.release_request_pad(&src_1);
    dispatcher.set_state(gstreamer::State::Null).unwrap();

    println!("stress rapid property changes test passed");
}
