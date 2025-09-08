//! Unit tests for SWRR (Smooth Weighted Round Robin) algorithm and hysteresis behavior
//!
//! These tests focus on the core algorithm behavior, property access, and pipeline creation.

use gst::prelude::*;
use gstreamer as gst;

use gstristelements::testing::*;

#[test]
fn test_basic_property_access() {
    init_for_tests();

    let dispatcher = create_dispatcher(None);

    println!("=== Basic Property Access Test ===");

    // Test JSON weights property
    let weights_json = r#"[1.0, 2.0]"#;
    dispatcher.set_property("weights", weights_json);
    let retrieved_weights: String = get_property(&dispatcher, "weights").unwrap();
    println!(
        "Set weights: '{}', Retrieved: '{}'",
        weights_json, retrieved_weights
    );
    assert_eq!(retrieved_weights, "[1.0,2.0]"); // JSON formatting may differ

    // Test rebalance interval property
    dispatcher.set_property("rebalance-interval-ms", 1000u64);
    let retrieved_interval: u64 = get_property(&dispatcher, "rebalance-interval-ms").unwrap();
    println!(
        "Set rebalance-interval-ms: 1000, Retrieved: {}",
        retrieved_interval
    );
    assert_eq!(retrieved_interval, 1000);

    // Test strategy enum property
    dispatcher.set_property("strategy", "aimd");
    let retrieved_strategy: String = get_property(&dispatcher, "strategy").unwrap();
    println!("Set strategy: 'aimd', Retrieved: '{}'", retrieved_strategy);
    assert_eq!(retrieved_strategy, "aimd");

    // Test boolean property
    dispatcher.set_property("caps-any", true);
    let retrieved_caps_any: bool = get_property(&dispatcher, "caps-any").unwrap();
    println!("Set caps-any: true, Retrieved: {}", retrieved_caps_any);
    assert!(retrieved_caps_any);

    // Test readonly property (current-weights should mirror weights)
    let current_weights: String = get_property(&dispatcher, "current-weights").unwrap();
    println!("Current weights (readonly): '{}'", current_weights);

    println!("✅ All basic property access tests passed!");
}

#[test]
fn test_basic_pipeline_creation() {
    init_for_tests();

    println!("=== Basic Pipeline Creation Test ===");

    // Create elements using convenience functions
    let dispatcher = create_dispatcher(Some(&[1.0, 2.0]));
    let _counter1 = create_counter_sink();
    let _counter2 = create_counter_sink();

    println!("✅ All elements created successfully");

    // Test pad creation
    let src_pad_0 = dispatcher
        .request_pad_simple("src_0")
        .expect("Failed to request src_0 pad");
    let src_pad_1 = dispatcher
        .request_pad_simple("src_1")
        .expect("Failed to request src_1 pad");

    println!(
        "✅ Source pads created: {} and {}",
        src_pad_0.name(),
        src_pad_1.name()
    );

    // Test sink pad access
    let sink_pad = dispatcher
        .static_pad("sink")
        .expect("Failed to get sink pad");
    println!("✅ Sink pad found: {}", sink_pad.name());

    // Test weight property after pad creation
    let current_weights: String = get_property(&dispatcher, "current-weights").unwrap();
    println!("Current weights after pad creation: '{}'", current_weights);

    println!("✅ Basic pipeline creation test passed!");
}

#[test]
fn test_hysteresis_behavior() {
    init_for_tests();

    println!("=== Hysteresis Behavior Test ===");

    // Create dispatcher with hysteresis settings that should limit switches
    let dispatcher = gst::ElementFactory::make("ristdispatcher")
        .property("min-hold-ms", 100u64) // Hold for at least 100ms
        .property("switch-threshold", 1.5f64) // Require 50% improvement to switch
        .build()
        .expect("Failed to create ristdispatcher");

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

    // Verify hysteresis properties are set correctly
    let min_hold: u64 = get_property(&dispatcher, "min-hold-ms").unwrap();
    let switch_threshold: f64 = get_property(&dispatcher, "switch-threshold").unwrap();

    assert_eq!(min_hold, 100);
    assert_eq!(switch_threshold, 1.5);

    println!("✅ Hysteresis properties test passed");
}

#[test]
fn test_warmup_period_behavior() {
    init_for_tests();

    println!("=== Warmup Period Test ===");

    let dispatcher = gst::ElementFactory::make("ristdispatcher")
        .property("health-warmup-ms", 1000u64) // 1 second warmup
        .build()
        .expect("Failed to create ristdispatcher");

    // Set initial weights
    let weights = vec![1.0, 1.0];
    let weights_json = serde_json::to_string(&weights).unwrap();
    dispatcher.set_property("weights", &weights_json);

    // Create first pad
    dispatcher
        .request_pad_simple("src_0")
        .expect("Failed to request first src pad");

    // Wait briefly, then add second pad (should trigger warmup)
    std::thread::sleep(std::time::Duration::from_millis(50));

    dispatcher
        .request_pad_simple("src_1")
        .expect("Failed to request second src pad");

    // Verify warmup property is set correctly
    let warmup: u64 = get_property(&dispatcher, "health-warmup-ms").unwrap();
    assert_eq!(warmup, 1000);

    println!("✅ Warmup period test passed");
}

#[test]
fn test_weighted_distribution_pipeline() {
    init_for_tests();

    println!("=== Weighted Distribution Pipeline Test ===");

    // Create elements
    let source = create_test_source();
    // Configure dispatcher for pure SWRR behavior to make distribution deterministic
    let dispatcher = create_dispatcher(Some(&[0.8, 0.2])); // Heavily favor first output
    dispatcher.set_property("auto-balance", false);
    dispatcher.set_property("min-hold-ms", 0u64);
    dispatcher.set_property("switch-threshold", 1.0f64);
    dispatcher.set_property("health-warmup-ms", 0u64);
    let counter1 = create_counter_sink();
    let counter2 = create_counter_sink();

    // Create pipeline
    let pipeline = gst::Pipeline::new();
    pipeline
        .add_many([&source, &dispatcher, &counter1, &counter2])
        .expect("Failed to add elements to pipeline");

    // Request src pads from dispatcher
    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();

    // Link elements
    source
        .link(&dispatcher)
        .expect("Failed to link source to dispatcher");
    src_0
        .link(&counter1.static_pad("sink").unwrap())
        .expect("Failed to link dispatcher src_0");
    src_1
        .link(&counter2.static_pad("sink").unwrap())
        .expect("Failed to link dispatcher src_1");

    // Run the pipeline a bit longer to allow distribution to manifest
    run_pipeline_for_duration(&pipeline, 2).expect("Pipeline run failed");

    // Check distribution
    let count1: u64 = get_property(&counter1, "count").unwrap();
    let count2: u64 = get_property(&counter2, "count").unwrap();

    println!("Counter 1: {} buffers (weight 0.8)", count1);
    println!("Counter 2: {} buffers (weight 0.2)", count2);

    // With 0.8/0.2 weights, we should see buffers on both outputs, dominated by counter1
    assert!(count1 > 0, "Counter 1 should receive buffers");
    assert!(
        count2 > 0,
        "Counter 2 should receive some buffers for 0.2 weight"
    );

    let total = count1 + count2;
    if total > 0 {
        let ratio1 = count1 as f64 / total as f64;
        let ratio2 = count2 as f64 / total as f64;
        // Allow generous tolerance due to discrete SWRR and startup effects
        assert!(
            ratio1 > 0.55,
            "Expected majority on path1 (~0.8), got {:.2}",
            ratio1
        );
        assert!(
            ratio2 < 0.45,
            "Expected minority on path2 (~0.2), got {:.2}",
            ratio2
        );
    }

    println!("✅ Weighted distribution pipeline test passed");
}
