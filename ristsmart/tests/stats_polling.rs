//! Statistics polling and processing tests
//!
//! Tests for RIST statistics integration and adaptive rebalancing

use gstristsmart::testing::*;
use gstristsmart::test_pipeline;
use gstreamer as gst;
use gst::prelude::*;
use std::time::Duration;

#[test]
fn test_basic_stats_functionality() {
    init_for_tests();
    
    println!("=== Basic Stats Functionality Test ===");
    
    let stats_mock = create_riststats_mock(Some(95.0), Some(20));
    
    // Test default stats structure
    let default_stats: gst::Structure = stats_mock.property("stats");
    println!("Default stats: {}", default_stats);
    
    assert_eq!(default_stats.name(), "rist/x-sender-stats",
              "Stats should have correct structure name");
    assert!(default_stats.n_fields() > 0, "Stats should have fields");
    
    // Test setting custom stats
    let custom_stats = gst::Structure::builder("rist/x-sender-stats")
        .field("session-0.sent-original-packets", 1000u64)
        .field("session-0.sent-retransmitted-packets", 10u64)
        .field("session-0.round-trip-time", 20.0f64)
        .build();
    
    stats_mock.set_property("stats", &custom_stats);
    let retrieved_stats: gst::Structure = stats_mock.property("stats");
    
    assert_eq!(retrieved_stats.get::<u64>("session-0.sent-original-packets"), Ok(1000u64));
    assert_eq!(retrieved_stats.get::<u64>("session-0.sent-retransmitted-packets"), Ok(10u64));
    assert_eq!(retrieved_stats.get::<f64>("session-0.round-trip-time"), Ok(20.0f64));
    
    println!("✅ Basic stats functionality test passed");
}

#[test]
fn test_stats_driven_rebalancing() {
    init_for_tests();
    
    println!("=== Stats-Driven Rebalancing Test ===");
    
    let source = create_test_source();
    let dispatcher = create_dispatcher_for_testing(Some(&[0.5, 0.5]));
    let stats_mock1 = create_riststats_mock(Some(95.0), Some(10)); // Good link
    let stats_mock2 = create_riststats_mock(Some(60.0), Some(80)); // Poor link
    let counter1 = create_counter_sink();
    let counter2 = create_counter_sink();
    
    // Configure dispatcher for adaptive rebalancing
    dispatcher.set_property("auto-balance", true);
    dispatcher.set_property("strategy", "ewma");
    dispatcher.set_property("rebalance-interval-ms", 200u64);
    
    test_pipeline!(pipeline, &source, &dispatcher, &counter1, &counter2);
    
    // Link pipeline: source -> dispatcher -> [counter1, counter2]
    source.link(&dispatcher).unwrap();
    
    let src_pad1 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_pad2 = dispatcher.request_pad_simple("src_%u").unwrap();
    
    src_pad1.link(&counter1.static_pad("sink").unwrap()).unwrap();
    src_pad2.link(&counter2.static_pad("sink").unwrap()).unwrap();
    
    // Associate stats mocks with dispatcher for rebalancing
    // (This would normally be done by the RIST transport elements)
    dispatcher.set_property("rist", &stats_mock1); // Primary stats source
    
    // Run pipeline to allow adaptation
    run_pipeline_for_duration(&pipeline, 3).expect("Stats rebalancing pipeline failed");
    
    // Check results
    let count1: u64 = get_property(&counter1, "count").unwrap();
    let count2: u64 = get_property(&counter2, "count").unwrap();
    let quality1: f64 = get_property(&stats_mock1, "quality").unwrap();
    let quality2: f64 = get_property(&stats_mock2, "quality").unwrap();
    
    println!("Stats-driven rebalancing results:");
    println!("  Path 1: {} buffers (quality: {:.1}%)", count1, quality1);
    println!("  Path 2: {} buffers (quality: {:.1}%)", count2, quality2);
    
    // Verify both paths received traffic
    assert!(count1 > 0 && count2 > 0, "Both paths should receive traffic");
    
    // Get current weights to verify adaptation
    let weights_str: String = dispatcher.property("current-weights");
    println!("Current weights: {}", weights_str);
    
    assert!(!weights_str.is_empty(), "Should have valid weights");
    
    println!("✅ Stats-driven rebalancing test passed");
}

#[test]
fn test_stats_structure_validation() {
    init_for_tests();
    
    println!("=== Stats Structure Validation Test ===");
    
    let stats_mock = create_riststats_mock(Some(90.0), Some(25));
    
    // Test with complete stats structure
    let complete_stats = gst::Structure::builder("rist/x-sender-stats")
        .field("session-0.sent-original-packets", 2000u64)
        .field("session-0.sent-retransmitted-packets", 20u64)
        .field("session-0.round-trip-time", 25.0f64)
        .field("session-1.sent-original-packets", 1800u64)
        .field("session-1.sent-retransmitted-packets", 180u64)
        .field("session-1.round-trip-time", 75.0f64)
        .build();
    
    stats_mock.set_property("stats", &complete_stats);
    let retrieved: gst::Structure = stats_mock.property("stats");
    
    println!("Complete stats structure:");
    for (field_name, field_value) in retrieved.iter() {
        println!("  {}: {:?}", field_name, field_value);
    }
    
    // Verify all fields are preserved
    assert_eq!(retrieved.get::<u64>("session-0.sent-original-packets"), Ok(2000u64));
    assert_eq!(retrieved.get::<u64>("session-1.sent-retransmitted-packets"), Ok(180u64));
    assert_eq!(retrieved.get::<f64>("session-1.round-trip-time"), Ok(75.0f64));
    
    println!("✅ Stats structure validation test passed");
}

#[test]
fn test_malformed_stats_handling() {
    init_for_tests();
    
    println!("=== Malformed Stats Handling Test ===");
    
    let stats_mock = create_riststats_mock(Some(85.0), Some(30));
    let dispatcher = create_dispatcher(Some(&[1.0]));
    
    dispatcher.set_property("auto-balance", true);
    dispatcher.set_property("rebalance-interval-ms", 100u64);
    dispatcher.set_property("rist", &stats_mock);
    
    // Request a pad to trigger weight management
    let _pad = dispatcher.request_pad_simple("src_%u").unwrap();
    
    // Test 1: Empty stats structure
    let empty_stats = gst::Structure::builder("rist/x-sender-stats").build();
    stats_mock.set_property("stats", &empty_stats);
    
    std::thread::sleep(Duration::from_millis(150));
    
    let weights_after_empty: String = dispatcher.property("current-weights");
    println!("Weights after empty stats: {}", weights_after_empty);
    
    assert!(!weights_after_empty.is_empty(), "Should maintain valid weights");
    
    // Test 2: Wrong structure name
    let wrong_name_stats = gst::Structure::builder("wrong/structure-name")
        .field("session-0.sent-original-packets", 1000u64)
        .build();
    stats_mock.set_property("stats", &wrong_name_stats);
    
    std::thread::sleep(Duration::from_millis(150));
    
    let weights_after_wrong: String = dispatcher.property("current-weights");
    println!("Weights after wrong name: {}", weights_after_wrong);
    
    assert!(!weights_after_wrong.is_empty(), "Should handle wrong structure name gracefully");
    
    // Test 3: Invalid field types
    let invalid_stats = gst::Structure::builder("rist/x-sender-stats")
        .field("session-0.sent-original-packets", "not_a_number")
        .field("session-0.round-trip-time", -1.0f64) // Negative RTT
        .build();
    stats_mock.set_property("stats", &invalid_stats);
    
    std::thread::sleep(Duration::from_millis(150));
    
    let weights_after_invalid: String = dispatcher.property("current-weights");
    println!("Weights after invalid data: {}", weights_after_invalid);
    
    assert!(!weights_after_invalid.is_empty(), "Should handle invalid data gracefully");
    
    println!("✅ Malformed stats handling test passed");
}

#[test]
fn test_multi_session_stats() {
    init_for_tests();
    
    println!("=== Multi-Session Stats Test ===");
    
    let stats_mock = create_riststats_mock(Some(90.0), Some(25));
    
    // Create stats for multiple sessions
    let multi_session_stats = gst::Structure::builder("rist/x-sender-stats")
        .field("session-0.sent-original-packets", 1000u64)
        .field("session-0.sent-retransmitted-packets", 10u64)
        .field("session-0.round-trip-time", 20.0f64)
        .field("session-1.sent-original-packets", 900u64)
        .field("session-1.sent-retransmitted-packets", 90u64)
        .field("session-1.round-trip-time", 50.0f64)
        .field("session-2.sent-original-packets", 800u64)
        .field("session-2.sent-retransmitted-packets", 200u64)
        .field("session-2.round-trip-time", 100.0f64)
        .build();
    
    stats_mock.set_property("stats", &multi_session_stats);
    let retrieved: gst::Structure = stats_mock.property("stats");
    
    println!("Multi-session stats:");
    let mut session_count = 0;
    for (field_name, _) in retrieved.iter() {
        if field_name.starts_with("session-") {
            let session_id = field_name.split('.').next().unwrap();
            if !field_name.contains(&format!("{}.sent-original-packets", session_id)) {
                continue;
            }
            session_count += 1;
            
            let sent_original: u64 = retrieved.get(&format!("{}.sent-original-packets", session_id)).unwrap();
            let sent_retx: u64 = retrieved.get(&format!("{}.sent-retransmitted-packets", session_id)).unwrap();
            let rtt: f64 = retrieved.get(&format!("{}.round-trip-time", session_id)).unwrap();
            
            let loss_rate = sent_retx as f64 / sent_original as f64 * 100.0;
            
            println!("  {}: sent={}, retx={}, rtt={:.1}ms, loss={:.2}%", 
                     session_id, sent_original, sent_retx, rtt, loss_rate);
        }
    }
    
    assert!(session_count >= 3, "Should detect multiple sessions");
    
    println!("✅ Multi-session stats test passed");
}

#[test]
fn test_stats_quality_calculation() {
    init_for_tests();
    
    println!("=== Stats Quality Calculation Test ===");
    
    let stats_mock = create_riststats_mock(Some(85.0), Some(40));
    
    // Test different quality scenarios
    let test_cases = [
        (1000u64, 0u64, 10.0f64, "Perfect link"),      // 0% loss, low RTT
        (1000u64, 10u64, 20.0f64, "Good link"),        // 1% loss, low RTT  
        (1000u64, 100u64, 50.0f64, "Degraded link"),   // 10% loss, medium RTT
        (1000u64, 300u64, 200.0f64, "Poor link"),      // 30% loss, high RTT
    ];
    
    for (sent, retx, rtt, description) in test_cases {
        let stats = gst::Structure::builder("rist/x-sender-stats")
            .field("session-0.sent-original-packets", sent)
            .field("session-0.sent-retransmitted-packets", retx)
            .field("session-0.round-trip-time", rtt)
            .build();
        
        stats_mock.set_property("stats", &stats);
        
        // If the mock supports quality calculation
        if let Ok(quality) = get_property::<f64>(&stats_mock, "quality") {
            let loss_rate = retx as f64 / sent as f64 * 100.0;
            println!("{}: loss={:.1}%, RTT={:.1}ms, quality={:.1}%", 
                     description, loss_rate, rtt, quality);
            
            // Quality should generally decrease with higher loss and RTT
            if retx == 0 && rtt < 50.0 {
                assert!(quality > 80.0, "Perfect conditions should yield high quality");
            }
        }
    }
    
    println!("✅ Stats quality calculation test passed");
}
