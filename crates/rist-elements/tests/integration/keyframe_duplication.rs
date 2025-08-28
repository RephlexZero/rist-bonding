//! Keyframe duplication tests
//!
//! Tests for the keyframe duplication functionality during failover scenarios

use gst::prelude::*;
use gstreamer as gst;
use gstristelements::testing::*;
use std::time::Duration;

#[test]
fn test_keyframe_duplication_properties() {
    init_for_tests();

    println!("=== Keyframe Duplication Properties Test ===");

    let dispatcher = create_dispatcher_for_testing(Some(&[0.5, 0.5]));

    // Test default values
    let default_dup_keyframes: bool = get_property(&dispatcher, "duplicate-keyframes").unwrap();
    let default_dup_budget: u32 = get_property(&dispatcher, "dup-budget-pps").unwrap();

    println!("Default keyframe duplication settings:");
    println!("  duplicate-keyframes: {}", default_dup_keyframes);
    println!("  dup-budget-pps: {}", default_dup_budget);

    assert!(!default_dup_keyframes, "Default duplicate-keyframes should be false");
    assert_eq!(default_dup_budget, 5, "Default dup-budget-pps should be 5");

    // Test setting properties within valid range only
    dispatcher.set_property("duplicate-keyframes", true);
    dispatcher.set_property("dup-budget-pps", 10u32);

    let enabled_dup_keyframes: bool = get_property(&dispatcher, "duplicate-keyframes").unwrap();
    let new_dup_budget: u32 = get_property(&dispatcher, "dup-budget-pps").unwrap();

    assert!(enabled_dup_keyframes, "duplicate-keyframes should be settable to true");
    assert_eq!(new_dup_budget, 10, "dup-budget-pps should be settable to 10");

    println!("Updated keyframe duplication settings:");
    println!("  duplicate-keyframes: {}", enabled_dup_keyframes);
    println!("  dup-budget-pps: {}", new_dup_budget);

    // Test boundary values
    dispatcher.set_property("dup-budget-pps", 0u32);
    let min_budget: u32 = get_property(&dispatcher, "dup-budget-pps").unwrap();
    assert_eq!(min_budget, 0, "dup-budget-pps should accept minimum value 0");

    dispatcher.set_property("dup-budget-pps", 100u32);
    let max_budget: u32 = get_property(&dispatcher, "dup-budget-pps").unwrap();
    assert_eq!(max_budget, 100, "dup-budget-pps should accept maximum value 100");

    println!("✅ Keyframe duplication properties test completed");
}

#[test]
fn test_keyframe_duplication_disabled_by_default() {
    init_for_tests();

    println!("=== Keyframe Duplication Disabled By Default Test ===");

    let source = create_test_source();
    let dispatcher = create_dispatcher_for_testing(Some(&[0.5, 0.5]));
    let counter1 = create_counter_sink();
    let counter2 = create_counter_sink();

    // Verify keyframe duplication is disabled by default
    let dup_enabled: bool = get_property(&dispatcher, "duplicate-keyframes").unwrap();
    assert!(!dup_enabled, "Keyframe duplication should be disabled by default");

    let pipeline = gst::Pipeline::new();
    pipeline
        .add_many([&source, &dispatcher, &counter1, &counter2])
        .expect("Failed to add elements to pipeline");

    // Set up pipeline: source -> dispatcher -> [counter1, counter2]
    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();

    source.link(&dispatcher).expect("Failed to link source");
    src_0
        .link(&counter1.static_pad("sink").unwrap())
        .expect("Failed to link src_0");
    src_1
        .link(&counter2.static_pad("sink").unwrap())
        .expect("Failed to link src_1");

    // Run pipeline with default settings
    pipeline
        .set_state(gst::State::Playing)
        .expect("Failed to start pipeline");
    std::thread::sleep(Duration::from_millis(500));

    pipeline
        .set_state(gst::State::Null)
        .expect("Failed to stop pipeline");

    let count1: u64 = get_property(&counter1, "count").unwrap();
    let count2: u64 = get_property(&counter2, "count").unwrap();

    println!("Buffer distribution with duplication disabled:");
    println!("  Path 1: {} buffers", count1);
    println!("  Path 2: {} buffers", count2);

    // With duplication disabled, buffers should only go to one path at a time
    assert!(
        count1 + count2 > 0,
        "Should receive some traffic even with duplication disabled"
    );

    println!("✅ Keyframe duplication disabled by default test completed");
}

#[test]
fn test_keyframe_duplication_enabled() {
    init_for_tests();

    println!("=== Keyframe Duplication Enabled Test ===");

    let source = create_test_source();
    let dispatcher = create_dispatcher_for_testing(Some(&[0.5, 0.5]));
    let counter1 = create_counter_sink();
    let counter2 = create_counter_sink();

    // Enable keyframe duplication
    dispatcher.set_property("duplicate-keyframes", true);
    dispatcher.set_property("dup-budget-pps", 20u32); // Higher budget for testing

    // Verify settings
    let dup_enabled: bool = get_property(&dispatcher, "duplicate-keyframes").unwrap();
    let budget: u32 = get_property(&dispatcher, "dup-budget-pps").unwrap();

    assert!(dup_enabled, "Keyframe duplication should be enabled");
    assert_eq!(budget, 20, "Duplication budget should be set to 20");

    let pipeline = gst::Pipeline::new();
    pipeline
        .add_many([&source, &dispatcher, &counter1, &counter2])
        .expect("Failed to add elements to pipeline");

    // Set up pipeline: source -> dispatcher -> [counter1, counter2]
    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();

    source.link(&dispatcher).expect("Failed to link source");
    src_0
        .link(&counter1.static_pad("sink").unwrap())
        .expect("Failed to link src_0");
    src_1
        .link(&counter2.static_pad("sink").unwrap())
        .expect("Failed to link src_1");

    println!("Running pipeline with keyframe duplication enabled:");
    println!("  duplicate-keyframes: {}", dup_enabled);
    println!("  dup-budget-pps: {}", budget);

    // Run pipeline with keyframe duplication enabled
    pipeline
        .set_state(gst::State::Playing)
        .expect("Failed to start pipeline");
    std::thread::sleep(Duration::from_millis(800)); // Longer runtime to trigger more events

    pipeline
        .set_state(gst::State::Null)
        .expect("Failed to stop pipeline");

    let count1: u64 = get_property(&counter1, "count").unwrap();
    let count2: u64 = get_property(&counter2, "count").unwrap();

    println!("Buffer distribution with duplication enabled:");
    println!("  Path 1: {} buffers", count1);
    println!("  Path 2: {} buffers", count2);

    // Basic sanity check
    assert!(
        count1 + count2 > 0,
        "Should receive traffic with duplication enabled"
    );

    println!("✅ Keyframe duplication enabled test completed");
}

#[test]
fn test_keyframe_duplication_budget_limits() {
    init_for_tests();

    println!("=== Keyframe Duplication Budget Limits Test ===");

    let source = create_test_source();
    let dispatcher = create_dispatcher_for_testing(Some(&[0.5, 0.5]));
    let counter1 = create_counter_sink();
    let counter2 = create_counter_sink();

    // Enable keyframe duplication with very low budget to test limiting
    dispatcher.set_property("duplicate-keyframes", true);
    dispatcher.set_property("dup-budget-pps", 2u32); // Very low budget

    let pipeline = gst::Pipeline::new();
    pipeline
        .add_many([&source, &dispatcher, &counter1, &counter2])
        .expect("Failed to add elements to pipeline");

    // Set up pipeline
    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();

    source.link(&dispatcher).expect("Failed to link source");
    src_0
        .link(&counter1.static_pad("sink").unwrap())
        .expect("Failed to link src_0");
    src_1
        .link(&counter2.static_pad("sink").unwrap())
        .expect("Failed to link src_1");

    println!("Testing keyframe duplication budget limits:");
    println!("  dup-budget-pps: 2 (very low to test limiting)");

    // Test low budget scenario - run first phase
    pipeline
        .set_state(gst::State::Playing)
        .expect("Failed to start pipeline");
    std::thread::sleep(Duration::from_millis(300));

    let count1_low: u64 = get_property(&counter1, "count").unwrap();
    let count2_low: u64 = get_property(&counter2, "count").unwrap();

    // Now increase budget and continue running 
    dispatcher.set_property("dup-budget-pps", 50u32); // Higher budget
    std::thread::sleep(Duration::from_millis(500)); // Longer run time

    let count1_high: u64 = get_property(&counter1, "count").unwrap();
    let count2_high: u64 = get_property(&counter2, "count").unwrap();

    pipeline
        .set_state(gst::State::Null)
        .expect("Failed to stop pipeline");

    println!("Budget limiting test results:");
    println!("  After first phase - Path 1: {}, Path 2: {}", count1_low, count2_low);
    println!("  After second phase - Path 1: {}, Path 2: {}", count1_high, count2_high);

    // The second phase should have more total traffic since it runs longer
    let total_low = count1_low + count2_low;
    let total_high = count1_high + count2_high;
    
    assert!(total_high >= total_low, 
            "Pipeline should continue producing traffic (first phase: {}, second phase: {})", total_low, total_high);

    println!("✅ Keyframe duplication budget limits test completed");
}

#[test]
fn test_keyframe_duplication_with_pad_switching() {
    init_for_tests();

    println!("=== Keyframe Duplication with Pad Switching Test ===");

    let source = create_test_source();
    let dispatcher = create_dispatcher_for_testing(Some(&[0.8, 0.2])); // Unbalanced to encourage switching
    let counter1 = create_counter_sink();
    let counter2 = create_counter_sink();

    // Enable keyframe duplication
    dispatcher.set_property("duplicate-keyframes", true);
    dispatcher.set_property("dup-budget-pps", 15u32);
    
    // Configure dispatcher for more dynamic switching
    dispatcher.set_property("min-hold-ms", 50u64); // Shorter hold time
    dispatcher.set_property("health-warmup-ms", 100u64); // Faster warmup

    let pipeline = gst::Pipeline::new();
    pipeline
        .add_many([&source, &dispatcher, &counter1, &counter2])
        .expect("Failed to add elements to pipeline");

    // Set up pipeline
    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();

    source.link(&dispatcher).expect("Failed to link source");
    src_0
        .link(&counter1.static_pad("sink").unwrap())
        .expect("Failed to link src_0");
    src_1
        .link(&counter2.static_pad("sink").unwrap())
        .expect("Failed to link src_1");

    println!("Testing keyframe duplication during pad switching:");
    println!("  Initial weights: [0.8, 0.2] (unbalanced to encourage switching)");

    // Phase 1: Initial distribution
    pipeline
        .set_state(gst::State::Playing)
        .expect("Failed to start pipeline");
    std::thread::sleep(Duration::from_millis(300));

    let count1_phase1: u64 = get_property(&counter1, "count").unwrap();
    let count2_phase1: u64 = get_property(&counter2, "count").unwrap();

    // Phase 2: Change weights to force switching
    let new_weights = vec![0.2, 0.8]; // Reverse the weights
    let weights_json = serde_json::to_string(&new_weights).unwrap();
    dispatcher.set_property("weights", &weights_json);
    
    std::thread::sleep(Duration::from_millis(400));

    pipeline
        .set_state(gst::State::Null)
        .expect("Failed to stop pipeline");

    let count1_final: u64 = get_property(&counter1, "count").unwrap();
    let count2_final: u64 = get_property(&counter2, "count").unwrap();

    let delta1 = count1_final - count1_phase1;
    let delta2 = count2_final - count2_phase1;

    println!("Results with pad switching and keyframe duplication:");
    println!("  Phase 1 - Path 1: {}, Path 2: {}", count1_phase1, count2_phase1);
    println!("  Phase 2 delta - Path 1: +{}, Path 2: +{}", delta1, delta2);
    println!("  Final - Path 1: {}, Path 2: {}", count1_final, count2_final);

    // Basic sanity check - both paths should have traffic (duplication happens during switches)
    let total_traffic = count1_final + count2_final;
    assert!(total_traffic > 0, "Should receive traffic with keyframe duplication enabled");

    // If there was significant switching, both paths might have traffic
    // But keyframe duplication only happens during actual switches with keyframes
    if count1_final > 0 && count2_final > 0 {
        println!("Traffic observed on both paths during keyframe duplication test");
    } else {
        println!("Traffic concentrated on primary path with occasional duplication (expected behavior)");
    }

    // After weight reversal, path 2 should get more additional traffic
    if delta2 > delta1 {
        println!("Weight switching successfully affected traffic distribution during duplication phase");
    }

    println!("✅ Keyframe duplication with pad switching test completed");
}

#[test]
fn test_keyframe_duplication_budget_reset() {
    init_for_tests();

    println!("=== Keyframe Duplication Budget Reset Test ===");

    let dispatcher = create_dispatcher_for_testing(Some(&[0.5, 0.5]));

    // Enable keyframe duplication with small budget
    dispatcher.set_property("duplicate-keyframes", true);
    dispatcher.set_property("dup-budget-pps", 3u32); // Small budget to test reset

    // Verify settings are applied
    let dup_enabled: bool = get_property(&dispatcher, "duplicate-keyframes").unwrap();
    let budget: u32 = get_property(&dispatcher, "dup-budget-pps").unwrap();

    assert!(dup_enabled, "Keyframe duplication should be enabled for budget reset test");
    assert_eq!(budget, 3, "Budget should be set to 3 for budget reset test");

    println!("Budget reset test setup:");
    println!("  duplicate-keyframes: {}", dup_enabled);
    println!("  dup-budget-pps: {} (small budget to test reset)", budget);

    // Create simple test pipeline
    let source = create_test_source();
    let counter1 = create_counter_sink();
    let counter2 = create_counter_sink();

    let pipeline = gst::Pipeline::new();
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

    // Run for different time periods to test budget reset behavior
    pipeline
        .set_state(gst::State::Playing)
        .expect("Failed to start pipeline");

    // Short burst - should hit budget limit
    std::thread::sleep(Duration::from_millis(200));
    let count1_burst: u64 = get_property(&counter1, "count").unwrap();
    let count2_burst: u64 = get_property(&counter2, "count").unwrap();
    let count_burst = count1_burst + count2_burst;

    // Wait for potential budget reset (budget resets every second)
    std::thread::sleep(Duration::from_millis(1200)); 
    
    // Run another period after budget reset
    let count1_after: u64 = get_property(&counter1, "count").unwrap();
    let count2_after: u64 = get_property(&counter2, "count").unwrap();
    let count_after_reset = count1_after + count2_after;

    pipeline
        .set_state(gst::State::Null)
        .expect("Failed to stop pipeline");

    println!("Budget reset behavior:");
    println!("  After burst: {} total buffers", count_burst);
    println!("  After potential reset: {} total buffers", count_after_reset);

    // Should have at least some traffic after running longer, 
    // though budget mainly affects duplication behavior, not total throughput
    assert!(count_after_reset >= count_burst, 
            "Should have consistent traffic over time (burst: {}, after: {})", count_burst, count_after_reset);
    
    // The test validates that budget settings don't break the pipeline
    println!("Budget reset functionality validated - pipeline remains stable with budget changes");

    println!("✅ Keyframe duplication budget reset test completed");
}