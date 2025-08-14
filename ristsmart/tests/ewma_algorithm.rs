//! EWMA (Exponentially Weighted Moving Average) algorithm tests
//!
//! Tests for the EWMA-based adaptive rebalancing strategy

use gstristsmart::testing::*;
use gstristsmart::test_pipeline;
use gstreamer as gst;
use gst::prelude::*;
use std::time::Duration;

#[test]
fn test_ewma_basic_functionality() {
    init_for_tests();
    
    println!("=== EWMA Basic Functionality Test ===");
    
    let dispatcher = create_dispatcher(Some(&[0.5, 0.5]));
    
    // Configure for EWMA strategy
    dispatcher.set_property("strategy", "ewma");
    dispatcher.set_property("rebalance-interval-ms", 100u64);
    dispatcher.set_property("auto-balance", true);
    
    // Verify strategy was set
    let strategy: String = get_property(&dispatcher, "strategy").unwrap();
    assert_eq!(strategy, "ewma", "Strategy should be set to EWMA");
    
    println!("✅ EWMA strategy configured successfully");
}

#[test]
fn test_ewma_with_mock_statistics() {
    init_for_tests();
    
    println!("=== EWMA with Mock Statistics Test ===");
    
    let source = create_test_source();
    let dispatcher = create_dispatcher(Some(&[0.5, 0.5]));
    let stats_mock1 = create_riststats_mock(Some(95.0), Some(10)); // Good link
    let stats_mock2 = create_riststats_mock(Some(50.0), Some(100)); // Poor link
    let counter1 = create_counter_sink();
    let counter2 = create_counter_sink();
    
    // Configure EWMA
    dispatcher.set_property("strategy", "ewma");
    dispatcher.set_property("rebalance-interval-ms", 200u64);
    dispatcher.set_property("auto-balance", true);
    
    test_pipeline!(pipeline, &source, &dispatcher, &stats_mock1, &stats_mock2,
                  &counter1, &counter2);
    
    // Create the pipeline: source -> dispatcher -> [stats_mock1 -> counter1, stats_mock2 -> counter2]
    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();
    
    source.link(&dispatcher).expect("Failed to link source to dispatcher");
    src_0.link(&stats_mock1.static_pad("sink").unwrap()).expect("Failed to link src_0");
    src_1.link(&stats_mock2.static_pad("sink").unwrap()).expect("Failed to link src_1");
    stats_mock1.link(&counter1).expect("Failed to link stats_mock1 to counter1");
    stats_mock2.link(&counter2).expect("Failed to link stats_mock2 to counter2");
    
    // Run pipeline to allow EWMA to adapt
    run_pipeline_for_duration(&pipeline, 3).expect("EWMA pipeline failed");
    
    // Check results
    let count1: u64 = get_property(&counter1, "count").unwrap();
    let count2: u64 = get_property(&counter2, "count").unwrap();
    let quality1: f64 = get_property(&stats_mock1, "quality").unwrap();
    let quality2: f64 = get_property(&stats_mock2, "quality").unwrap();
    let rtt1: u32 = get_property(&stats_mock1, "rtt").unwrap();
    let rtt2: u32 = get_property(&stats_mock2, "rtt").unwrap();
    
    println!("EWMA adaptation results:");
    println!("  Path 1: {} buffers (quality: {:.1}%, RTT: {}ms)", count1, quality1, rtt1);
    println!("  Path 2: {} buffers (quality: {:.1}%, RTT: {}ms)", count2, quality2, rtt2);
    
    // EWMA should eventually favor the better link (path 1)
    if count1 + count2 > 20 { // Only check if we have enough samples
        let ratio = count1 as f64 / (count1 + count2) as f64;
        println!("  Traffic ratio to better path: {:.2}", ratio);
        
        // With significant quality difference, EWMA should favor the better path
        if quality1 > quality2 + 20.0 {
            assert!(ratio > 0.6, "EWMA should favor the better quality path");
            println!("✅ EWMA correctly adapted to favor better link");
        } else {
            println!("ⓘ Quality difference not significant enough for strong adaptation");
        }
    }
    
    println!("✅ EWMA with mock statistics test completed");
}

#[test]
fn test_ewma_adaptation_over_time() {
    init_for_tests();
    
    println!("=== EWMA Time-based Adaptation Test ===");
    
    let source = create_test_source();
    let dispatcher = create_dispatcher(Some(&[0.5, 0.5]));
    let stats_mock1 = create_riststats_mock(Some(90.0), Some(20));
    let stats_mock2 = create_riststats_mock(Some(90.0), Some(20)); // Initially equal
    let counter1 = create_counter_sink();
    let counter2 = create_counter_sink();
    
    // Configure EWMA with shorter intervals for testing
    dispatcher.set_property("strategy", "ewma");
    dispatcher.set_property("rebalance-interval-ms", 100u64);
    dispatcher.set_property("auto-balance", true);
    
    test_pipeline!(pipeline, &source, &dispatcher, &stats_mock1, &stats_mock2,
                  &counter1, &counter2);
    
    // Set up pipeline
    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();
    
    source.link(&dispatcher).expect("Failed to link source");
    src_0.link(&stats_mock1.static_pad("sink").unwrap()).expect("Failed to link src_0");
    src_1.link(&stats_mock2.static_pad("sink").unwrap()).expect("Failed to link src_1");
    stats_mock1.link(&counter1).expect("Failed to link to counter1");
    stats_mock2.link(&counter2).expect("Failed to link to counter2");
    
    // Phase 1: Equal conditions
    println!("Phase 1: Equal link conditions");
    pipeline.set_state(gst::State::Playing).expect("Failed to start pipeline");
    std::thread::sleep(Duration::from_secs(1));
    
    let count1_phase1: u64 = get_property(&counter1, "count").unwrap();
    let count2_phase1: u64 = get_property(&counter2, "count").unwrap();
    
    println!("  Phase 1 - Path 1: {} buffers, Path 2: {} buffers", 
             count1_phase1, count2_phase1);
    
    // Phase 2: Degrade path 2
    println!("Phase 2: Degrading path 2");
    stats_mock2.set_property("quality", 30.0); // Significantly worse
    stats_mock2.set_property("rtt", 200u32);   // Higher latency
    
    std::thread::sleep(Duration::from_millis(500)); // Allow adaptation time
    
    let count1_phase2: u64 = get_property(&counter1, "count").unwrap();
    let count2_phase2: u64 = get_property(&counter2, "count").unwrap();
    
    let delta1 = count1_phase2 - count1_phase1;
    let delta2 = count2_phase2 - count2_phase1;
    
    println!("  Phase 2 - Path 1: +{} buffers, Path 2: +{} buffers", delta1, delta2);
    
    // Phase 3: Recovery
    println!("Phase 3: Path 2 recovery");
    stats_mock2.set_property("quality", 95.0); // Better than path 1
    stats_mock2.set_property("rtt", 10u32);    // Lower latency
    
    std::thread::sleep(Duration::from_millis(500)); // Allow adaptation time
    
    pipeline.set_state(gst::State::Null).expect("Failed to stop pipeline");
    
    let count1_final: u64 = get_property(&counter1, "count").unwrap();
    let count2_final: u64 = get_property(&counter2, "count").unwrap();
    
    let delta1_recovery = count1_final - count1_phase2;
    let delta2_recovery = count2_final - count2_phase2;
    
    println!("  Phase 3 - Path 1: +{} buffers, Path 2: +{} buffers", 
             delta1_recovery, delta2_recovery);
    
    // Verify EWMA adapted during degradation phase
    if delta1 + delta2 > 5 { // Enough samples to analyze
        let phase2_ratio = delta1 as f64 / (delta1 + delta2) as f64;
        println!("  Degradation phase ratio (favor path 1): {:.2}", phase2_ratio);
        
        // During degradation, EWMA should have favored path 1
        assert!(phase2_ratio > 0.6, "EWMA should adapt during link degradation");
    }
    
    println!("Final traffic distribution:");
    println!("  Path 1 (good->equal): {} total buffers", count1_final);
    println!("  Path 2 (good->bad->best): {} total buffers", count2_final);
    
    println!("✅ EWMA time-based adaptation test completed");
}

#[test]
fn test_ewma_vs_fixed_weights() {
    init_for_tests();
    
    println!("=== EWMA vs Fixed Weights Comparison ===");
    
    // Test 1: Fixed weight dispatcher
    let dispatcher_fixed = create_dispatcher(Some(&[0.8, 0.2])); // Fixed 80/20 split
    dispatcher_fixed.set_property("auto-balance", false);
    
    // Test 2: EWMA dispatcher
    let dispatcher_ewma = create_dispatcher(Some(&[0.5, 0.5])); // Start equal
    dispatcher_ewma.set_property("strategy", "ewma");
    dispatcher_ewma.set_property("auto-balance", true);
    dispatcher_ewma.set_property("rebalance-interval-ms", 100u64);
    
    println!("Created fixed-weight and EWMA dispatchers for comparison");
    
    // Verify configurations
    let auto_balance_fixed: bool = get_property(&dispatcher_fixed, "auto-balance").unwrap();
    let auto_balance_ewma: bool = get_property(&dispatcher_ewma, "auto-balance").unwrap();
    let strategy_ewma: String = get_property(&dispatcher_ewma, "strategy").unwrap();
    
    assert!(!auto_balance_fixed, "Fixed dispatcher should not auto-balance");
    assert!(auto_balance_ewma, "EWMA dispatcher should auto-balance");
    assert_eq!(strategy_ewma, "ewma", "EWMA strategy should be configured");
    
    println!("Dispatcher configurations:");
    println!("  Fixed: auto-balance={}", auto_balance_fixed);
    println!("  EWMA: auto-balance={}, strategy={}", auto_balance_ewma, strategy_ewma);
    
    println!("✅ EWMA vs Fixed weights comparison test completed");
}

#[test]
fn test_ewma_parameter_tuning() {
    init_for_tests();
    
    println!("=== EWMA Parameter Tuning Test ===");
    
    let dispatcher = create_dispatcher(Some(&[0.5, 0.5]));
    
    // Configure EWMA with different parameters
    dispatcher.set_property("strategy", "ewma");
    dispatcher.set_property("auto-balance", true);
    
    // Test different rebalance intervals
    for interval in [50u64, 100u64, 200u64, 500u64] {
        dispatcher.set_property("rebalance-interval-ms", interval);
        let actual_interval: u64 = get_property(&dispatcher, "rebalance-interval-ms").unwrap();
        assert_eq!(actual_interval, interval, 
                  "Rebalance interval should be configurable");
        println!("  Interval {}ms: ✓", interval);
    }
    
    // Verify current configuration
    let final_strategy: String = get_property(&dispatcher, "strategy").unwrap();
    let final_auto_balance: bool = get_property(&dispatcher, "auto-balance").unwrap();
    let final_interval: u64 = get_property(&dispatcher, "rebalance-interval-ms").unwrap();
    
    println!("Final EWMA configuration:");
    println!("  Strategy: {}", final_strategy);
    println!("  Auto-balance: {}", final_auto_balance);
    println!("  Rebalance interval: {}ms", final_interval);
    
    assert_eq!(final_strategy, "ewma");
    assert!(final_auto_balance);
    
    println!("✅ EWMA parameter tuning test completed");
}
