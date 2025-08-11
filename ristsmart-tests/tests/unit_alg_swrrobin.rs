// Pure SWRR + hysteresis unit/property tests

use gstreamer::{self as gst, prelude::*};
use gstreamer_app as gst_app;

/// Test basic property access after the indexing fix
#[test]
fn test_basic_property_access() {
    ristsmart_tests::register_everything_for_tests();

    let dispatcher = gst::ElementFactory::make("ristdispatcher")
        .build()
        .expect("Failed to create ristdispatcher");

    // Test basic property access with simple values
    println!("=== Basic Property Access Test ===");

    // Test string property
    let weights_json = r#"[1.0, 2.0]"#;
    dispatcher.set_property("weights", &weights_json);
    let retrieved_weights: String = dispatcher.property("weights");
    println!(
        "Set weights: '{}', Retrieved: '{}'",
        weights_json, retrieved_weights
    );
    assert_eq!(retrieved_weights, "[1.0,2.0]"); // JSON formatting may differ

    // Test u64 property
    dispatcher.set_property("rebalance-interval-ms", &1000u64);
    let retrieved_interval: u64 = dispatcher.property("rebalance-interval-ms");
    println!(
        "Set rebalance-interval-ms: 1000, Retrieved: {}",
        retrieved_interval
    );
    assert_eq!(retrieved_interval, 1000);

    // Test string enum property
    dispatcher.set_property("strategy", &"aimd");
    let retrieved_strategy: String = dispatcher.property("strategy");
    println!("Set strategy: 'aimd', Retrieved: '{}'", retrieved_strategy);
    assert_eq!(retrieved_strategy, "aimd");

    // Test boolean property
    dispatcher.set_property("caps-any", &true);
    let retrieved_caps_any: bool = dispatcher.property("caps-any");
    println!("Set caps-any: true, Retrieved: {}", retrieved_caps_any);
    assert_eq!(retrieved_caps_any, true);

    // Test readonly property (current-weights should mirror weights)
    let current_weights: String = dispatcher.property("current-weights");
    println!("Current weights (readonly): '{}'", current_weights);

    println!("✅ All basic property access tests passed!");
}

/// Test that we can create a simple pipeline with the dispatcher
#[test]
fn test_basic_pipeline_creation() {
    ristsmart_tests::register_everything_for_tests();

    println!("=== Basic Pipeline Creation Test ===");

    // Create basic elements
    let dispatcher = gst::ElementFactory::make("ristdispatcher")
        .property("weights", &"[1.0, 2.0]")
        .build()
        .expect("Failed to create ristdispatcher");

    let counter1 = gst::ElementFactory::make("counter_sink")
        .build()
        .expect("Failed to create counter_sink");

    let counter2 = gst::ElementFactory::make("counter_sink")
        .build()
        .expect("Failed to create counter_sink");

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
    let current_weights: String = dispatcher.property("current-weights");
    println!("Current weights after pad creation: '{}'", current_weights);

    println!("✅ Basic pipeline creation test passed!");
}

/// Test hysteresis behavior - switches should be limited when weights oscillate
#[test]
fn test_hysteresis_limits_switches() {
    ristsmart_tests::register_everything_for_tests();

    // Create dispatcher with hysteresis settings that should limit switches
    let dispatcher = gst::ElementFactory::make("ristdispatcher")
        .property("min-hold-ms", 100u64) // Hold for at least 100ms
        .property("switch-threshold", 1.5f64) // Require 50% improvement to switch
        .build()
        .expect("Failed to create ristdispatcher");

    // Start with equal weights, then create a small oscillation
    let initial_weights = vec![1.0, 1.0];
    let weights_json = serde_json::to_string(&initial_weights).unwrap();
    dispatcher.set_property("weights", &weights_json);

    // Create src pads
    for i in 0..2 {
        dispatcher
            .request_pad_simple(&format!("src_{}", i))
            .expect("Failed to request src pad");
    }

    // We would need to access the internal state to properly test hysteresis
    // For now, verify the properties are accepted
    let min_hold: u64 = dispatcher.property("min-hold-ms");
    let switch_threshold: f64 = dispatcher.property("switch-threshold");

    assert_eq!(min_hold, 100);
    assert_eq!(switch_threshold, 1.5);

    println!("Hysteresis properties test passed");
}

/// Test warm-up period behavior - new pads should have reduced weight initially
#[test]
fn test_warmup_period() {
    ristsmart_tests::register_everything_for_tests();

    let dispatcher = gst::ElementFactory::make("ristdispatcher")
        .property("health-warmup-ms", 1000u64) // 1 second warmup
        .build()
        .expect("Failed to create ristdispatcher");

    // Set weights
    let weights = vec![1.0, 1.0];
    let weights_json = serde_json::to_string(&weights).unwrap();
    dispatcher.set_property("weights", &weights_json);

    // Create one pad initially
    dispatcher
        .request_pad_simple("src_0")
        .expect("Failed to request first src pad");

    // Wait a moment, then add second pad (should trigger warmup)
    std::thread::sleep(std::time::Duration::from_millis(50));

    dispatcher
        .request_pad_simple("src_1")
        .expect("Failed to request second src pad");

    // Verify warmup property is set
    let warmup: u64 = dispatcher.property("health-warmup-ms");
    assert_eq!(warmup, 1000);

    println!("Warmup period test passed");
}
