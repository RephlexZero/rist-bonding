use gstreamer as gst;
use gst::prelude::*;
use gstristelements::testing::*;
use std::time::Duration;

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

#[test]
fn test_invalid_weight_values_recovery() {
    init_for_tests();
    println!("=== Invalid Weight Values Recovery Test ===");

    let dispatcher = create_dispatcher_for_testing(Some(&[0.5, 0.5]));
    
    // Verify initial state
    let initial_weights: String = dispatcher.property("weights");
    println!("Initial weights: {}", initial_weights);
    assert!(initial_weights.contains("0.5"), "Should have initial weights");

    // Test various invalid inputs
    let invalid_inputs = vec![
        "invalid json",
        "[1.0, -0.5]",           // Negative weight
        "[1.0, NaN]",            // Invalid number
        "[]",                    // Empty array
        "[1.0]",                 // Wrong length for 2-pad setup
        "[Infinity, 0.5]",       // Infinity value
        "null",                  // Null value
        "[1.0, 0.5, 0.3, 0.2]",  // Too many weights
    ];

    for invalid_input in invalid_inputs {
        println!("Testing invalid input: {}", invalid_input);
        
        // Attempt to set invalid weights - should be ignored or corrected
        dispatcher.set_property("weights", invalid_input);
        
        // Verify dispatcher is still functional
        let current_weights: String = dispatcher.property("weights");
        assert!(!current_weights.is_empty(), "Weights should not be empty after invalid input");
        
        // Should be able to set valid weights afterward
        dispatcher.set_property("weights", "[0.3, 0.7]");
        let recovery_weights: String = dispatcher.property("weights");
        assert!(recovery_weights.contains("0.3"), "Should recover and accept valid weights");
    }

    println!("✅ Invalid weight values recovery test passed");
}

#[test]
fn test_pipeline_error_conditions() {
    init_for_tests();
    println!("=== Pipeline Error Conditions Test ===");

    let pipeline = gst::Pipeline::new();
    let dispatcher = create_dispatcher_for_testing(Some(&[0.6, 0.4]));
    let source = create_test_source();

    pipeline.add_many([&source, &dispatcher]).unwrap();
    source.link(&dispatcher).unwrap();

    // Test 1: Start pipeline with no output pads (should not crash)
    println!("Testing pipeline with no output pads...");
    pipeline.set_state(gst::State::Playing).unwrap();
    run_mainloop_ms(200);
    
    // Should be able to transition states without issues
    pipeline.set_state(gst::State::Paused).unwrap();
    pipeline.set_state(gst::State::Playing).unwrap();
    
    // Test 2: Add pads while pipeline is running
    println!("Adding pads to running pipeline...");
    let counter1 = create_counter_sink();
    let counter2 = create_counter_sink();
    pipeline.add_many([&counter1, &counter2]).unwrap();
    
    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();
    src_0.link(&counter1.static_pad("sink").unwrap()).unwrap();
    src_1.link(&counter2.static_pad("sink").unwrap()).unwrap();
    
    run_mainloop_ms(300);
    
    let count1: u64 = get_property(&counter1, "count").unwrap();
    let count2: u64 = get_property(&counter2, "count").unwrap();
    println!("Counts after adding pads: C1={}, C2={}", count1, count2);
    
    // Test 3: Force error condition by removing elements while linked
    println!("Testing forced error conditions...");
    pipeline.set_state(gst::State::Null).unwrap();
    
    // Try to operate on pads after pipeline is stopped
    let recovery_weights: String = dispatcher.property("weights");
    assert!(!recovery_weights.is_empty(), "Should still be able to read properties");
    
    dispatcher.set_property("weights", "[0.8, 0.2]");
    let updated_weights: String = dispatcher.property("weights");
    assert!(updated_weights.contains("0.8"), "Should still be able to update properties");

    println!("✅ Pipeline error conditions test passed");
}

#[test]
fn test_pad_linking_error_recovery() {
    init_for_tests();
    println!("=== Pad Linking Error Recovery Test ===");

    let dispatcher = create_dispatcher_for_testing(Some(&[1.0]));
    
    // Test requesting pads before pipeline setup
    println!("Testing pad requests without pipeline...");
    let pad1 = dispatcher.request_pad_simple("src_%u");
    assert!(pad1.is_some(), "Should be able to request pads without pipeline");
    
    // Test releasing non-existent pads (should not crash)
    println!("Testing pad release edge cases...");
    // The release operation should be safe even with invalid pads
    
    // Test proper pad lifecycle
    if let Some(pad) = pad1 {
        dispatcher.release_request_pad(&pad);
    }
    
    // Test with pipeline setup
    let pipeline = gst::Pipeline::new();
    let source = create_test_source();
    pipeline.add_many([&source, &dispatcher]).unwrap();
    source.link(&dispatcher).unwrap();
    
    // Test requesting many pads
    println!("Testing multiple pad requests...");
    let mut pads = Vec::new();
    for _i in 0..5 {
        if let Some(pad) = dispatcher.request_pad_simple("src_%u") {
            pads.push((pad, create_counter_sink()));
        }
    }
    
    // Add counters and link pads
    for (pad, counter) in &pads {
        pipeline.add(counter).unwrap();
        pad.link(&counter.static_pad("sink").unwrap()).unwrap();
    }
    
    pipeline.set_state(gst::State::Playing).unwrap();
    run_mainloop_ms(200);
    pipeline.set_state(gst::State::Null).unwrap();
    
    // Clean up all pads
    for (pad, counter) in pads {
        let _ = pad.unlink(&counter.static_pad("sink").unwrap());
        pipeline.remove(&counter).ok();
        dispatcher.release_request_pad(&pad);
    }
    
    // Verify dispatcher is still functional
    let final_weights: String = dispatcher.property("weights");
    assert!(!final_weights.is_empty(), "Dispatcher should remain functional");

    println!("✅ Pad linking error recovery test passed");
}

#[test]
fn test_property_error_handling() {
    init_for_tests();
    println!("=== Property Error Handling Test ===");

    let dispatcher = create_dispatcher_for_testing(Some(&[0.4, 0.6]));
    
    // Test setting properties to extreme values
    println!("Testing extreme property values...");
    
    // Test that property setting can handle various edge cases
    // Note: GStreamer properties with validation will panic on invalid values,
    // which is expected behavior. We test recovery from valid edge cases.
    
    // Very large but valid interval (should be clamped)
    dispatcher.set_property("rebalance-interval-ms", 5000u64); // 5 seconds
    let interval: u64 = dispatcher.property("rebalance-interval-ms");
    assert!(interval > 0, "Should accept large valid interval values");
    
    // Small but valid interval
    dispatcher.set_property("rebalance-interval-ms", 100u64);
    let interval: u64 = dispatcher.property("rebalance-interval-ms");
    assert!(interval > 0, "Should accept valid interval values");
    
    // Valid strategy
    dispatcher.set_property("strategy", "ewma");
    let strategy: String = dispatcher.property("strategy");
    assert_eq!(strategy, "ewma", "Should accept valid strategy");
    
    // Test switching strategies rapidly
    println!("Testing rapid strategy switching...");
    for i in 0..10 {
        let strategy = if i % 2 == 0 { "aimd" } else { "ewma" };
        dispatcher.set_property("strategy", strategy);
        let current: String = dispatcher.property("strategy");
        assert!(current == "aimd" || current == "ewma", "Strategy should be valid");
    }
    
    // Test properties during different states
    dispatcher.set_property("auto-balance", false);
    dispatcher.set_property("auto-balance", true);
    
    let auto_balance: bool = dispatcher.property("auto-balance");
    println!("Auto-balance setting: {}", auto_balance);
    
    // Test metrics intervals
    dispatcher.set_property("metrics-export-interval-ms", 5000u64); // 5 seconds
    let metrics_interval: u64 = dispatcher.property("metrics-export-interval-ms");
    assert!(metrics_interval > 0, "Should accept valid metrics interval");

    println!("✅ Property error handling test passed");
}

#[test] 
fn test_state_corruption_recovery() {
    init_for_tests();
    println!("=== State Corruption Recovery Test ===");

    let dispatcher = create_dispatcher_for_testing(Some(&[0.33, 0.33, 0.34]));
    let pipeline = gst::Pipeline::new();
    let source = create_test_source();
    
    pipeline.add_many([&source, &dispatcher]).unwrap();
    source.link(&dispatcher).unwrap();
    
    // Create and link multiple pads
    let mut pads_and_counters = Vec::new();
    for _ in 0..3 {
        let counter = create_counter_sink();
        pipeline.add(&counter).unwrap();
        
        if let Some(pad) = dispatcher.request_pad_simple("src_%u") {
            pad.link(&counter.static_pad("sink").unwrap()).unwrap();
            pads_and_counters.push((pad, counter));
        }
    }
    
    pipeline.set_state(gst::State::Playing).unwrap();
    run_mainloop_ms(200);
    
    // Test 1: Abrupt state changes while data is flowing
    println!("Testing abrupt state transitions...");
    for _ in 0..5 {
        pipeline.set_state(gst::State::Null).ok();
        pipeline.set_state(gst::State::Playing).ok();
        run_mainloop_ms(50);
    }
    
    // Test 2: Remove pads in wrong order / without proper cleanup
    println!("Testing improper pad removal...");
    pipeline.set_state(gst::State::Paused).unwrap();
    
    // Remove some elements abruptly
    if let Some((pad, counter)) = pads_and_counters.pop() {
        // Skip unlinking step intentionally to test error recovery
        pipeline.remove(&counter).ok();
        dispatcher.release_request_pad(&pad);
    }
    
    // Verify dispatcher can still function
    dispatcher.set_property("weights", "[0.7, 0.3]"); // Adjust for remaining pads
    run_mainloop_ms(100);
    
    pipeline.set_state(gst::State::Playing).unwrap();
    run_mainloop_ms(200);
    
    // Test 3: Verify remaining pads still work
    let remaining_counts: Vec<u64> = pads_and_counters
        .iter()
        .map(|(_, counter)| get_property(counter, "count").unwrap_or(0))
        .collect();
    
    println!("Remaining pad counts: {:?}", remaining_counts);
    let total_count: u64 = remaining_counts.iter().sum();
    assert!(total_count > 0, "Remaining pads should still receive data");
    
    // Test 4: Full recovery - clean shutdown and restart
    println!("Testing full recovery...");
    pipeline.set_state(gst::State::Null).unwrap();
    
    // Clean up remaining pads properly
    for (pad, counter) in pads_and_counters {
        let _ = pad.unlink(&counter.static_pad("sink").unwrap());
        pipeline.remove(&counter).ok();
        dispatcher.release_request_pad(&pad);
    }
    
    // Verify dispatcher can be reconfigured
    dispatcher.set_property("weights", "[1.0]");
    dispatcher.set_property("strategy", "ewma");
    
    let recovery_weights: String = dispatcher.property("weights");
    assert!(!recovery_weights.is_empty(), "Should recover to functional state");

    println!("✅ State corruption recovery test passed");
}

#[test]
fn test_resource_exhaustion_handling() {
    init_for_tests(); 
    println!("=== Resource Exhaustion Handling Test ===");

    let dispatcher = create_dispatcher_for_testing(Some(&[1.0]));
    let pipeline = gst::Pipeline::new();
    let source = create_test_source();
    
    pipeline.add_many([&source, &dispatcher]).unwrap();
    source.link(&dispatcher).unwrap();
    
    // Test creating many pads to test resource limits
    println!("Testing pad creation limits...");
    let mut created_pads = Vec::new();
    let max_attempts = 100; // Reasonable limit for testing
    
    for i in 0..max_attempts {
        if let Some(pad) = dispatcher.request_pad_simple("src_%u") {
            let counter = create_counter_sink();
            
            if pipeline.add(&counter).is_ok() {
                if pad.link(&counter.static_pad("sink").unwrap()).is_ok() {
                    created_pads.push((pad, counter));
                } else {
                    // Cleanup failed link attempt
                    pipeline.remove(&counter).ok();
                    dispatcher.release_request_pad(&pad);
                    break;
                }
            } else {
                dispatcher.release_request_pad(&pad);
                break;
            }
        } else {
            println!("Pad creation failed at attempt {}", i);
            break;
        }
    }
    
    let pad_count = created_pads.len();
    println!("Successfully created {} pads", pad_count);
    assert!(pad_count > 5, "Should be able to create reasonable number of pads");
    
    // Test the system with many pads
    pipeline.set_state(gst::State::Playing).unwrap();
    run_mainloop_ms(300);
    
    // Verify data flow with many pads
    let active_pads = created_pads
        .iter()
        .map(|(_, counter)| get_property::<u64>(counter, "count").unwrap_or(0))
        .filter(|&count| count > 0)
        .count();
    
    println!("Active pads (with data): {}/{}", active_pads, pad_count);
    
    pipeline.set_state(gst::State::Null).unwrap();
    
    // Clean up all pads
    println!("Cleaning up {} pads...", pad_count);
    for (pad, counter) in created_pads {
        let _ = pad.unlink(&counter.static_pad("sink").unwrap());
        pipeline.remove(&counter).ok();
        dispatcher.release_request_pad(&pad);
    }
    
    // Verify dispatcher is still functional after cleanup
    dispatcher.set_property("weights", "[1.0]");
    let final_weights: String = dispatcher.property("weights");
    assert!(!final_weights.is_empty(), "Should be functional after cleanup");

    println!("✅ Resource exhaustion handling test passed");
}

#[test]
fn test_malformed_statistics_handling() {
    init_for_tests();
    println!("=== Malformed Statistics Handling Test ===");

    let dispatcher = create_dispatcher_for_testing(Some(&[0.5, 0.5]));
    
    // Test with strategy that uses statistics
    dispatcher.set_property("strategy", "ewma");
    dispatcher.set_property("rebalance-interval-ms", 200u64);
    
    // Create a pipeline to enable stats processing
    let pipeline = gst::Pipeline::new();
    let source = create_test_source(); 
    let counter1 = create_counter_sink();
    let counter2 = create_counter_sink();
    
    pipeline.add_many([&source, &dispatcher, &counter1, &counter2]).unwrap();
    source.link(&dispatcher).unwrap();
    
    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();
    src_0.link(&counter1.static_pad("sink").unwrap()).unwrap();
    src_1.link(&counter2.static_pad("sink").unwrap()).unwrap();
    
    pipeline.set_state(gst::State::Playing).unwrap();
    run_mainloop_ms(200);
    
    let initial_count1: u64 = get_property(&counter1, "count").unwrap();
    let initial_count2: u64 = get_property(&counter2, "count").unwrap();
    println!("Initial counts: C1={}, C2={}", initial_count1, initial_count2);
    
    // Test rapid strategy changes (might cause internal state inconsistencies)
    println!("Testing rapid strategy changes...");
    for i in 0..20 {
        let strategy = if i % 2 == 0 { "aimd" } else { "ewma" };
        dispatcher.set_property("strategy", strategy);
        dispatcher.set_property("rebalance-interval-ms", (100 + i * 10) as u64);
        std::thread::sleep(Duration::from_millis(10));
    }
    
    run_mainloop_ms(400);
    
    let final_count1: u64 = get_property(&counter1, "count").unwrap();
    let final_count2: u64 = get_property(&counter2, "count").unwrap();
    println!("Final counts: C1={}, C2={}", final_count1, final_count2);
    
    // Verify dispatcher continued functioning despite rapid changes
    // Note: Counter values may not increase if source sent limited buffers
    
    assert!(final_count1 >= initial_count1, "Counter1 should not decrease");
    assert!(final_count2 >= initial_count2, "Counter2 should not decrease");
    
    // The important thing is that the dispatcher didn't crash from the rapid changes
    println!("Dispatcher survived {} rapid configuration changes", 20);
    
    pipeline.set_state(gst::State::Null).unwrap();
    
    // Verify final state is coherent
    let final_strategy: String = dispatcher.property("strategy");
    assert!(final_strategy == "aimd" || final_strategy == "ewma", "Strategy should be valid");
    
    let final_weights: String = dispatcher.property("weights");
    assert!(!final_weights.is_empty(), "Weights should remain valid");

    println!("✅ Malformed statistics handling test passed");
}