//! Dispatcher metrics bus message tests
//!
//! Tests for the metrics export functionality that should emit bus messages
//! with dispatcher statistics at configured intervals

use gst::prelude::*;
use gstreamer as gst;
use gstristelements::testing::*;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Pump the GLib main loop for the specified duration
fn run_mainloop_ms(ms: u64) {
    // Pump the default GLib main context where timeout_add registered
    let ctx = glib::MainContext::default();
    let _guard = ctx.acquire().expect("acquire main context");
    let end = std::time::Instant::now() + std::time::Duration::from_millis(ms);
    while std::time::Instant::now() < end {
        // Drain all pending events without blocking, then sleep briefly.
        while ctx.iteration(false) {}
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
fn test_metrics_export_disabled_by_default() {
    init_for_tests();

    println!("=== Metrics Export Disabled By Default Test ===");

    let dispatcher = create_dispatcher_for_testing(Some(&[0.5, 0.5]));

    // Test default value
    let default_interval: u64 = get_property(&dispatcher, "metrics-export-interval-ms").unwrap();
    println!("Default metrics export interval: {}ms", default_interval);

    assert_eq!(
        default_interval, 0,
        "Default metrics export should be disabled (0ms)"
    );

    println!("✅ Metrics export disabled by default test completed");
}

#[test]
fn test_metrics_export_properties() {
    init_for_tests();

    println!("=== Metrics Export Properties Test ===");

    let dispatcher = create_dispatcher_for_testing(Some(&[0.5, 0.5]));

    println!("Testing property ranges and defaults...");

    // Test default value
    let default_interval: u64 = get_property(&dispatcher, "metrics-export-interval-ms").unwrap();
    println!("Default metrics export interval: {}ms", default_interval);

    // Test setting different intervals individually with error handling
    let test_intervals = [100u64, 500u64, 1000u64, 5000u64];

    for &interval in &test_intervals {
        println!("Attempting to set metrics interval to: {}ms", interval);

        match std::panic::catch_unwind(|| {
            dispatcher.set_property("metrics-export-interval-ms", interval);
            get_property::<u64>(&dispatcher, "metrics-export-interval-ms").unwrap_or(0)
        }) {
            Ok(actual_interval) => {
                println!("  Successfully set to: {}ms", actual_interval);
                assert_eq!(
                    actual_interval, interval,
                    "Should be able to set interval to {}",
                    interval
                );
            }
            Err(_) => {
                println!(
                    "  Failed to set interval {}, checking if property exists",
                    interval
                );
                // Try to read the property to confirm it exists
                if let Ok(current) = get_property::<u64>(&dispatcher, "metrics-export-interval-ms")
                {
                    println!("  Property exists with current value: {}", current);
                } else {
                    println!("  Property may not exist!");
                }
                panic!("Failed to set metrics-export-interval-ms to {}", interval);
            }
        }
    }

    // Test boundary values
    dispatcher.set_property("metrics-export-interval-ms", 0u64);
    let disabled_interval: u64 = get_property(&dispatcher, "metrics-export-interval-ms").unwrap();
    assert_eq!(disabled_interval, 0, "Should accept 0 (disabled)");

    dispatcher.set_property("metrics-export-interval-ms", 60000u64);
    let max_interval: u64 = get_property(&dispatcher, "metrics-export-interval-ms").unwrap();
    assert_eq!(max_interval, 60000, "Should accept maximum value 60000ms");

    // Test value rejection for out-of-range (should fail to set, not clamp)
    let before_invalid = get_property::<u64>(&dispatcher, "metrics-export-interval-ms").unwrap();

    let invalid_result = std::panic::catch_unwind(|| {
        dispatcher.set_property("metrics-export-interval-ms", 70000u64);
    });

    // The property set should fail for out-of-range values
    assert!(
        invalid_result.is_err(),
        "Should reject out-of-range value 70000"
    );

    // Property should retain its previous value
    let after_invalid = get_property::<u64>(&dispatcher, "metrics-export-interval-ms").unwrap();
    assert_eq!(
        after_invalid, before_invalid,
        "Property should retain previous value after invalid set"
    );

    println!("Final metrics export interval: {}ms", after_invalid);
    println!("✅ Metrics export properties test completed");
}

#[test]
fn test_metrics_bus_message_structure() {
    init_for_tests();

    println!("=== Metrics Bus Message Structure Test ===");

    // Create pipeline with dispatcher
    let pipeline = gst::Pipeline::new();
    let source = create_test_source();
    let dispatcher = create_dispatcher_for_testing(Some(&[0.7, 0.3]));
    let counter1 = create_counter_sink();
    let counter2 = create_counter_sink();

    pipeline
        .add_many([&source, &dispatcher, &counter1, &counter2])
        .expect("Failed to add elements to pipeline");

    // Set up pipeline
    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();

    source.link(&dispatcher).expect("Failed to link source");
    println!("Successfully linked source to dispatcher");

    // Verify the sinkpad was created
    if let Some(sinkpad) = dispatcher.static_pad("sink") {
        println!("Dispatcher has sink pad: {}", sinkpad.name());
    } else {
        println!("WARNING: Dispatcher has no sink pad!");
    }

    src_0
        .link(&counter1.static_pad("sink").unwrap())
        .expect("Failed to link src_0");
    src_1
        .link(&counter2.static_pad("sink").unwrap())
        .expect("Failed to link src_1");

    // Set up bus message collection BEFORE setting metrics property
    let bus = pipeline.bus().expect("Pipeline should have a bus");
    let messages: Arc<Mutex<Vec<gst::Structure>>> = Arc::new(Mutex::new(Vec::new()));
    let messages_clone = messages.clone();

    // Install bus watch
    let _watch_id = bus.add_watch(move |_bus, message| {
        match message.type_() {
            gst::MessageType::Application => {
                if let Some(structure) = message.structure() {
                    if structure.name() == "rist-dispatcher-metrics" {
                        println!("Received metrics message: {}", structure.to_string());
                        messages_clone.lock().unwrap().push(structure.to_owned());
                    }
                }
            }
            _ => {}
        }
        glib::ControlFlow::Continue
    });

    // Start pipeline FIRST
    println!("Starting pipeline with metrics export enabled (500ms interval)");
    pipeline
        .set_state(gst::State::Playing)
        .expect("Failed to start pipeline");

    // Give pipeline time to start up
    std::thread::sleep(Duration::from_millis(100));

    // THEN enable metrics export with short interval
    dispatcher.set_property("metrics-export-interval-ms", 500u64);

    // Wait for metrics messages (should get 2-3 messages in 1.2 seconds) with main loop pumping
    println!("Pumping main loop for 1200ms to allow timer callbacks...");
    run_mainloop_ms(1200);

    pipeline
        .set_state(gst::State::Null)
        .expect("Failed to stop pipeline");

    // Check collected messages
    let collected_messages = messages.lock().unwrap();
    println!("Collected {} metrics messages", collected_messages.len());

    // We should have at least one metrics message
    assert!(
        collected_messages.len() > 0,
        "Should have received at least one metrics message"
    );

    // Check structure of first message
    if let Some(first_message) = collected_messages.first() {
        println!("Analyzing first metrics message structure:");

        // Check expected fields
        let expected_fields = [
            "timestamp",
            "current-weights",
            "buffers-processed",
            "src-pad-count",
            "selected-index",
        ];

        for field in &expected_fields {
            assert!(
                first_message.has_field(*field),
                "Metrics message should have '{}' field",
                field
            );
            println!("  ✓ Found field: {}", field);
        }

        // Validate field types and values
        if let Ok(timestamp) = first_message.get::<u64>("timestamp") {
            assert!(timestamp > 0, "Timestamp should be positive");
            println!("  ✓ Timestamp: {}", timestamp);
        }

        if let Ok(weights) = first_message.get::<String>("current-weights") {
            println!("  ✓ Current weights: {}", weights);
            // Should be valid JSON array
            assert!(
                weights.starts_with('[') && weights.ends_with(']'),
                "Weights should be JSON array format"
            );
        }

        if let Ok(pad_count) = first_message.get::<u32>("src-pad-count") {
            assert_eq!(pad_count, 2, "Should report 2 source pads");
            println!("  ✓ Source pad count: {}", pad_count);
        }

        if let Ok(selected_index) = first_message.get::<u32>("selected-index") {
            assert!(selected_index < 2, "Selected index should be valid (< 2)");
            println!("  ✓ Selected index: {}", selected_index);
        }
    }

    println!("✅ Metrics bus message structure test completed");
}

#[test]
fn test_metrics_export_timing() {
    init_for_tests();

    println!("=== Metrics Export Timing Test ===");

    // Create simple pipeline
    let pipeline = gst::Pipeline::new();
    let source = create_test_source();
    let dispatcher = create_dispatcher_for_testing(Some(&[0.5, 0.5]));
    let counter = create_counter_sink();

    pipeline
        .add_many([&source, &dispatcher, &counter])
        .expect("Failed to add elements to pipeline");

    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    source.link(&dispatcher).expect("Failed to link source");
    src_0
        .link(&counter.static_pad("sink").unwrap())
        .expect("Failed to link src_0");

    // Set short metrics interval for timing test
    dispatcher.set_property("metrics-export-interval-ms", 300u64);

    // Set up timing tracking
    let message_times = Arc::new(Mutex::new(Vec::new()));
    let message_times_clone = message_times.clone();

    let bus = pipeline.bus().expect("Pipeline should have a bus");
    let _watch_id = bus.add_watch(move |_bus, message| {
        if let gst::MessageType::Application = message.type_() {
            if let Some(structure) = message.structure() {
                if structure.name() == "rist-dispatcher-metrics" {
                    let now = std::time::Instant::now();
                    message_times_clone.lock().unwrap().push(now);
                    println!("Received metrics message at: {:?}", now);
                }
            }
        }
        glib::ControlFlow::Continue
    });

    println!("Testing metrics timing with 300ms interval");
    let start_time = std::time::Instant::now();

    pipeline
        .set_state(gst::State::Playing)
        .expect("Failed to start pipeline");

    // Run for about 1 second to collect several messages
    run_mainloop_ms(1100);

    pipeline
        .set_state(gst::State::Null)
        .expect("Failed to stop pipeline");

    let end_time = std::time::Instant::now();
    let total_duration = end_time.duration_since(start_time);

    // Check timing
    let times = message_times.lock().unwrap();
    println!(
        "Collected {} messages over {:?}",
        times.len(),
        total_duration
    );

    // Should have received approximately 1100ms / 300ms = 3-4 messages
    assert!(
        times.len() >= 2,
        "Should have received at least 2 messages in ~1.1s with 300ms interval"
    );
    assert!(
        times.len() <= 6,
        "Should not have too many messages (got {})",
        times.len()
    );

    // Check intervals between messages (should be roughly 300ms)
    if times.len() >= 2 {
        for i in 1..times.len() {
            let interval = times[i].duration_since(times[i - 1]);
            println!("  Interval {}: {:?}", i, interval);

            // Allow some tolerance (200ms - 500ms range)
            assert!(
                interval >= Duration::from_millis(200),
                "Interval {} should be at least 200ms, got {:?}",
                i,
                interval
            );
            assert!(
                interval <= Duration::from_millis(500),
                "Interval {} should be at most 500ms, got {:?}",
                i,
                interval
            );
        }
    }

    println!("✅ Metrics export timing test completed");
}

#[test]
fn test_metrics_export_enable_disable() {
    init_for_tests();

    println!("=== Metrics Export Enable/Disable Test ===");

    // Create pipeline
    let pipeline = gst::Pipeline::new();
    let source = create_test_source();
    let dispatcher = create_dispatcher_for_testing(Some(&[0.5, 0.5]));
    let counter = create_counter_sink();

    pipeline
        .add_many([&source, &dispatcher, &counter])
        .expect("Failed to add elements to pipeline");

    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    source.link(&dispatcher).expect("Failed to link source");
    src_0
        .link(&counter.static_pad("sink").unwrap())
        .expect("Failed to link src_0");

    // Initially disabled
    let message_count = Arc::new(Mutex::new(0u32));
    let message_count_clone = message_count.clone();

    let bus = pipeline.bus().expect("Pipeline should have a bus");
    let _watch_id = bus.add_watch(move |_bus, message| {
        if let gst::MessageType::Application = message.type_() {
            if let Some(structure) = message.structure() {
                if structure.name() == "rist-dispatcher-metrics" {
                    *message_count_clone.lock().unwrap() += 1;
                }
            }
        }
        glib::ControlFlow::Continue
    });

    pipeline
        .set_state(gst::State::Playing)
        .expect("Failed to start pipeline");

    // Phase 1: Disabled (default)
    println!("Phase 1: Metrics disabled (should receive no messages)");
    run_mainloop_ms(400);
    let count_disabled = *message_count.lock().unwrap();

    // Phase 2: Enable metrics
    println!("Phase 2: Enabling metrics with 200ms interval");
    dispatcher.set_property("metrics-export-interval-ms", 200u64);
    run_mainloop_ms(600); // Should get ~3 messages
    let count_enabled = *message_count.lock().unwrap();

    // Phase 3: Disable again
    println!("Phase 3: Disabling metrics again");
    dispatcher.set_property("metrics-export-interval-ms", 0u64);
    run_mainloop_ms(400);
    let count_disabled_again = *message_count.lock().unwrap();

    pipeline
        .set_state(gst::State::Null)
        .expect("Failed to stop pipeline");

    println!("Message counts:");
    println!("  Disabled phase: {}", count_disabled);
    println!(
        "  Enabled phase: {} (delta: {})",
        count_enabled,
        count_enabled - count_disabled
    );
    println!(
        "  Disabled again: {} (delta: {})",
        count_disabled_again,
        count_disabled_again - count_enabled
    );

    // Validate behavior
    assert_eq!(
        count_disabled, 0,
        "Should receive no messages when disabled"
    );
    assert!(
        count_enabled > count_disabled,
        "Should receive messages when enabled"
    );
    assert_eq!(
        count_disabled_again, count_enabled,
        "Should stop receiving messages when disabled again"
    );

    println!("✅ Metrics export enable/disable test completed");
}

#[test]
fn test_metrics_with_dynamic_weights() {
    init_for_tests();

    println!("=== Metrics with Dynamic Weights Test ===");

    // Create pipeline
    let pipeline = gst::Pipeline::new();
    let source = create_test_source();
    let dispatcher = create_dispatcher_for_testing(Some(&[0.8, 0.2]));
    let counter1 = create_counter_sink();
    let counter2 = create_counter_sink();

    pipeline
        .add_many([&source, &dispatcher, &counter1, &counter2])
        .expect("Failed to add elements to pipeline");

    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();
    source.link(&dispatcher).expect("Failed to link source");
    src_0
        .link(&counter1.static_pad("sink").unwrap())
        .expect("Failed to link src_0");
    src_1
        .link(&counter2.static_pad("sink").unwrap())
        .expect("Failed to link src_1");

    // Enable metrics
    dispatcher.set_property("metrics-export-interval-ms", 300u64);

    // Track weight changes in metrics
    let weight_changes = Arc::new(Mutex::new(Vec::new()));
    let weight_changes_clone = weight_changes.clone();

    let bus = pipeline.bus().expect("Pipeline should have a bus");
    let _watch_id = bus.add_watch(move |_bus, message| {
        if let gst::MessageType::Application = message.type_() {
            if let Some(structure) = message.structure() {
                if structure.name() == "rist-dispatcher-metrics" {
                    if let Ok(weights_str) = structure.get::<String>("current-weights") {
                        weight_changes_clone.lock().unwrap().push(weights_str);
                    }
                }
            }
        }
        glib::ControlFlow::Continue
    });

    pipeline
        .set_state(gst::State::Playing)
        .expect("Failed to start pipeline");

    // Let initial metrics be captured
    run_mainloop_ms(400);

    // Change weights dynamically
    println!("Changing weights from [0.8, 0.2] to [0.3, 0.7]");
    let new_weights = vec![0.3, 0.7];
    let weights_json = serde_json::to_string(&new_weights).unwrap();
    dispatcher.set_property("weights", &weights_json);

    // Wait for more metrics
    run_mainloop_ms(600);

    pipeline
        .set_state(gst::State::Null)
        .expect("Failed to stop pipeline");

    // Check weight changes were captured in metrics
    let captured_weights = weight_changes.lock().unwrap();
    println!("Captured {} weight snapshots:", captured_weights.len());

    for (i, weights) in captured_weights.iter().enumerate() {
        println!("  Snapshot {}: {}", i, weights);
    }

    assert!(
        captured_weights.len() >= 2,
        "Should have captured at least 2 weight snapshots"
    );

    // Should have initial weights and changed weights
    let initial_weights = &captured_weights[0];
    let final_weights = captured_weights.last().unwrap();

    // Verify weight change was captured
    assert_ne!(
        initial_weights, final_weights,
        "Should have captured weight change in metrics"
    );

    println!("✅ Metrics with dynamic weights test completed");
}
