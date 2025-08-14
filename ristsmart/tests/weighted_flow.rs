//! Weighted flow distribution tests
//!
//! Tests for weighted buffer distribution across multiple output paths

use gstristsmart::testing::*;
use gstristsmart::test_pipeline;
use gstreamer as gst;
use gst::prelude::*;
use std::time::Duration;

#[test]
fn test_weighted_flow_distribution() {
    init_for_tests();
    
    println!("=== Weighted Flow Distribution Test ===");
    
    // Create pipeline: source -> dispatcher -> 3 counter_sinks
    let source = create_test_source();
    let dispatcher = create_dispatcher(Some(&[3.0, 2.0, 1.0])); // 50%, 33%, 17% expected
    let counter1 = create_counter_sink();
    let counter2 = create_counter_sink();
    let counter3 = create_counter_sink();
    
    // Disable auto-balance for deterministic testing
    dispatcher.set_property("auto-balance", false);
    
    test_pipeline!(pipeline, &source, &dispatcher, &counter1, &counter2, &counter3);
    
    // Link elements
    source.link(&dispatcher).expect("Failed to link source to dispatcher");
    
    let src_pad1 = dispatcher.request_pad_simple("src_%u").expect("Failed to request pad 1");
    let src_pad2 = dispatcher.request_pad_simple("src_%u").expect("Failed to request pad 2");
    let src_pad3 = dispatcher.request_pad_simple("src_%u").expect("Failed to request pad 3");
    
    src_pad1.link(&counter1.static_pad("sink").unwrap()).expect("Failed to link to counter1");
    src_pad2.link(&counter2.static_pad("sink").unwrap()).expect("Failed to link to counter2");
    src_pad3.link(&counter3.static_pad("sink").unwrap()).expect("Failed to link to counter3");
    
    // Run pipeline for data distribution
    run_pipeline_for_duration(&pipeline, 3).expect("Weighted flow pipeline failed");
    
    // Check buffer distribution
    let count1: u64 = get_property(&counter1, "count").unwrap();
    let count2: u64 = get_property(&counter2, "count").unwrap();
    let count3: u64 = get_property(&counter3, "count").unwrap();
    let total = count1 + count2 + count3;
    
    println!("Buffer distribution with weights [3.0, 2.0, 1.0]:");
    println!("  Counter 1: {} buffers ({:.1}%)", count1, count1 as f64 / total as f64 * 100.0);
    println!("  Counter 2: {} buffers ({:.1}%)", count2, count2 as f64 / total as f64 * 100.0);
    println!("  Counter 3: {} buffers ({:.1}%)", count3, count3 as f64 / total as f64 * 100.0);
    println!("  Total: {} buffers", total);
    
    // Verify we got reasonable traffic
    assert!(total > 10, "Should receive reasonable amount of traffic");
    
    // Verify approximate weighted distribution (allowing 10% tolerance)
    let ratio1 = count1 as f64 / total as f64;
    let ratio2 = count2 as f64 / total as f64;
    let ratio3 = count3 as f64 / total as f64;
    
    // Expected ratios: 3:2:1 = 50%, 33.33%, 16.67%
    assert!((ratio1 - 0.5).abs() < 0.1, 
           "Counter1 should receive ~50% of buffers, got {:.1}%", ratio1 * 100.0);
    assert!((ratio2 - 0.3333).abs() < 0.1,
           "Counter2 should receive ~33% of buffers, got {:.1}%", ratio2 * 100.0);
    assert!((ratio3 - 0.1667).abs() < 0.1,
           "Counter3 should receive ~17% of buffers, got {:.1}%", ratio3 * 100.0);
    
    // Verify ordering by weight
    assert!(count1 > count2, "Higher weight should receive more buffers");
    assert!(count2 > count3, "Higher weight should receive more buffers");
    
    println!("✅ Weighted flow distribution test passed");
}

#[test]
fn test_equal_weight_distribution() {
    init_for_tests();
    
    println!("=== Equal Weight Distribution Test ===");
    
    let source = create_test_source();
    let dispatcher = create_dispatcher(Some(&[1.0, 1.0, 1.0])); // Equal weights
    let counter1 = create_counter_sink();
    let counter2 = create_counter_sink();
    let counter3 = create_counter_sink();
    
    dispatcher.set_property("auto-balance", false);
    
    test_pipeline!(pipeline, &source, &dispatcher, &counter1, &counter2, &counter3);
    
    // Link elements
    source.link(&dispatcher).expect("Failed to link source");
    
    let src_pad1 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_pad2 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_pad3 = dispatcher.request_pad_simple("src_%u").unwrap();
    
    src_pad1.link(&counter1.static_pad("sink").unwrap()).unwrap();
    src_pad2.link(&counter2.static_pad("sink").unwrap()).unwrap();
    src_pad3.link(&counter3.static_pad("sink").unwrap()).unwrap();
    
    // Run pipeline
    run_pipeline_for_duration(&pipeline, 3).expect("Equal weight pipeline failed");
    
    let count1: u64 = get_property(&counter1, "count").unwrap();
    let count2: u64 = get_property(&counter2, "count").unwrap();
    let count3: u64 = get_property(&counter3, "count").unwrap();
    let total = count1 + count2 + count3;
    
    println!("Equal weight distribution [1.0, 1.0, 1.0]:");
    println!("  Counter 1: {} buffers", count1);
    println!("  Counter 2: {} buffers", count2);
    println!("  Counter 3: {} buffers", count3);
    
    assert!(total > 10, "Should receive reasonable traffic");
    
    // With equal weights, distribution should be roughly even (±30% tolerance)
    let avg = total as f64 / 3.0;
    let tolerance = avg * 0.3;
    
    assert!((count1 as f64 - avg).abs() < tolerance,
           "Counter1 should be close to average with equal weights");
    assert!((count2 as f64 - avg).abs() < tolerance,
           "Counter2 should be close to average with equal weights");
    assert!((count3 as f64 - avg).abs() < tolerance,
           "Counter3 should be close to average with equal weights");
    
    println!("✅ Equal weight distribution test passed");
}

#[test]
fn test_dynamic_weight_adjustment() {
    init_for_tests();
    
    println!("=== Dynamic Weight Adjustment Test ===");
    
    let source = create_test_source();
    let dispatcher = create_dispatcher(Some(&[1.0, 1.0])); // Start equal
    let counter1 = create_counter_sink();
    let counter2 = create_counter_sink();
    
    dispatcher.set_property("auto-balance", false);
    
    test_pipeline!(pipeline, &source, &dispatcher, &counter1, &counter2);
    
    // Link elements
    source.link(&dispatcher).unwrap();
    
    let src_pad1 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_pad2 = dispatcher.request_pad_simple("src_%u").unwrap();
    
    src_pad1.link(&counter1.static_pad("sink").unwrap()).unwrap();
    src_pad2.link(&counter2.static_pad("sink").unwrap()).unwrap();
    
    // Phase 1: Run with equal weights
    pipeline.set_state(gst::State::Playing).unwrap();
    std::thread::sleep(Duration::from_secs(1));
    
    let count1_phase1: u64 = get_property(&counter1, "count").unwrap();
    let count2_phase1: u64 = get_property(&counter2, "count").unwrap();
    
    println!("Phase 1 (equal weights): Counter1={}, Counter2={}", 
             count1_phase1, count2_phase1);
    
    // Phase 2: Adjust weights to favor counter2
    dispatcher.set_property_from_str("weights", "[1.0, 4.0]");
    
    std::thread::sleep(Duration::from_secs(1));
    
    let count1_phase2: u64 = get_property(&counter1, "count").unwrap();
    let count2_phase2: u64 = get_property(&counter2, "count").unwrap();
    
    // Calculate increments
    let delta1 = count1_phase2 - count1_phase1;
    let delta2 = count2_phase2 - count2_phase1;
    
    println!("Phase 2 (adjusted weights [1.0, 4.0]): Counter1=+{}, Counter2=+{}", 
             delta1, delta2);
    
    pipeline.set_state(gst::State::Null).unwrap();
    
    // Verify that weight adjustment had effect
    if delta1 + delta2 > 10 { // Enough samples to analyze
        assert!(delta2 > delta1, 
               "Counter2 should receive more traffic after weight increase");
        
        // With 1:4 ratio, counter2 should get significantly more
        if delta1 > 0 {
            let ratio = delta2 as f64 / delta1 as f64;
            println!("Traffic ratio in phase 2: {:.2}", ratio);
            assert!(ratio > 1.5, "Weight adjustment should significantly affect traffic distribution");
        }
    }
    
    println!("✅ Dynamic weight adjustment test passed");
}

#[test]
fn test_extreme_weight_ratios() {
    init_for_tests();
    
    println!("=== Extreme Weight Ratios Test ===");
    
    let source = create_test_source();
    let dispatcher = create_dispatcher(Some(&[10.0, 0.1])); // 100:1 ratio
    let counter1 = create_counter_sink();
    let counter2 = create_counter_sink();
    
    dispatcher.set_property("auto-balance", false);
    
    test_pipeline!(pipeline, &source, &dispatcher, &counter1, &counter2);
    
    // Link elements
    source.link(&dispatcher).unwrap();
    
    let src_pad1 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_pad2 = dispatcher.request_pad_simple("src_%u").unwrap();
    
    src_pad1.link(&counter1.static_pad("sink").unwrap()).unwrap();
    src_pad2.link(&counter2.static_pad("sink").unwrap()).unwrap();
    
    // Run pipeline
    run_pipeline_for_duration(&pipeline, 3).expect("Extreme weights pipeline failed");
    
    let count1: u64 = get_property(&counter1, "count").unwrap();
    let count2: u64 = get_property(&counter2, "count").unwrap();
    let total = count1 + count2;
    
    println!("Extreme weight ratio [10.0, 0.1]:");
    println!("  Counter 1 (high weight): {} buffers ({:.1}%)", 
             count1, count1 as f64 / total as f64 * 100.0);
    println!("  Counter 2 (low weight): {} buffers ({:.1}%)", 
             count2, count2 as f64 / total as f64 * 100.0);
    
    assert!(total > 10, "Should receive traffic");
    
    // Counter 1 should get the vast majority (>80%)
    let ratio1 = count1 as f64 / total as f64;
    assert!(ratio1 > 0.8, 
           "High weight counter should dominate traffic distribution, got {:.1}%", 
           ratio1 * 100.0);
    
    // Counter 2 should still get some traffic (not completely starved)
    assert!(count2 > 0, "Low weight counter should still receive some traffic");
    
    println!("✅ Extreme weight ratios test passed");
}

#[test]
fn test_zero_weight_handling() {
    init_for_tests();
    
    println!("=== Zero Weight Handling Test ===");
    
    let source = create_test_source();
    let dispatcher = create_dispatcher(Some(&[1.0, 0.0, 1.0])); // Middle weight is zero
    let counter1 = create_counter_sink();
    let counter2 = create_counter_sink();
    let counter3 = create_counter_sink();
    
    dispatcher.set_property("auto-balance", false);
    
    test_pipeline!(pipeline, &source, &dispatcher, &counter1, &counter2, &counter3);
    
    // Link elements
    source.link(&dispatcher).unwrap();
    
    let src_pad1 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_pad2 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_pad3 = dispatcher.request_pad_simple("src_%u").unwrap();
    
    src_pad1.link(&counter1.static_pad("sink").unwrap()).unwrap();
    src_pad2.link(&counter2.static_pad("sink").unwrap()).unwrap();
    src_pad3.link(&counter3.static_pad("sink").unwrap()).unwrap();
    
    // Run pipeline
    run_pipeline_for_duration(&pipeline, 3).expect("Zero weight pipeline failed");
    
    let count1: u64 = get_property(&counter1, "count").unwrap();
    let count2: u64 = get_property(&counter2, "count").unwrap();
    let count3: u64 = get_property(&counter3, "count").unwrap();
    
    println!("Zero weight handling [1.0, 0.0, 1.0]:");
    println!("  Counter 1: {} buffers", count1);
    println!("  Counter 2 (zero weight): {} buffers", count2);
    println!("  Counter 3: {} buffers", count3);
    
    // Counter 2 should receive no or minimal traffic due to zero weight
    let total_active = count1 + count3;
    if total_active > 0 {
        let zero_ratio = count2 as f64 / (count1 + count2 + count3) as f64;
        assert!(zero_ratio < 0.1, 
               "Zero weight counter should receive minimal traffic, got {:.1}%", 
               zero_ratio * 100.0);
    }
    
    // Traffic should be split between counter1 and counter3
    assert!(count1 > 0 || count3 > 0, "Non-zero weight counters should receive traffic");
    
    println!("✅ Zero weight handling test passed");
}

#[test]
fn test_weight_normalization() {
    init_for_tests();
    
    println!("=== Weight Normalization Test ===");
    
    // Test that large weights get normalized properly
    let dispatcher = create_dispatcher(Some(&[1000.0, 2000.0, 3000.0]));
    dispatcher.set_property("auto-balance", false);
    
    // The actual weights used should maintain the same ratios
    // but be normalized to reasonable values
    
    let current_weights: String = dispatcher.property("current-weights");
    println!("Normalized weights: {}", current_weights);
    
    // Just verify that the element doesn't crash with large weights
    // and that the property can be read back
    assert!(!current_weights.is_empty(), "Should be able to read back weights");
    
    println!("✅ Weight normalization test passed");
}
