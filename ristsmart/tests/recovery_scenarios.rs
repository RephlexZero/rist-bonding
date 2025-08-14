//! Network recovery and degradation scenario tests
//!
//! These tests simulate real-world network conditions including degradation,
//! recovery, and multiple failure/recovery cycles to ensure robust behavior.

use gstristsmart::testing::*;
use gstristsmart::test_pipeline;
use gstreamer as gst;
use gst::prelude::*;

#[test]
fn test_single_link_degradation_recovery() {
    init_for_tests();

    println!("=== Single Link Degradation Recovery Test ===");

    // Create elements
    let dispatcher = create_dispatcher(Some(&[0.5, 0.5]));
    let mock_stats = create_mock_stats(2);
    let counter1 = create_counter_sink();
    let counter2 = create_counter_sink();

    // Set up initial good conditions
    mock_stats.tick(&[1000, 1000], &[10, 10], &[25, 25]);

    // Create a simple data flow test
    let source = create_test_source();
    test_pipeline!(pipeline, &source, &dispatcher, &counter1, &counter2);

    // Link elements
    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();

    source.link(&dispatcher).expect("Failed to link source");
    src_0.link(&counter1.static_pad("sink").unwrap()).expect("Failed to link src_0");
    src_1.link(&counter2.static_pad("sink").unwrap()).expect("Failed to link src_1");

    // Phase 1: Normal operation
    println!("Phase 1: Normal operation");
    run_pipeline_for_duration(&pipeline, 1).expect("Phase 1 failed");

    let phase1_count1: u64 = get_property(&counter1, "count").unwrap();
    let phase1_count2: u64 = get_property(&counter2, "count").unwrap();
    println!("Phase 1 - Counter 1: {}, Counter 2: {}", phase1_count1, phase1_count2);

    // Phase 2: Degrade link 0
    println!("Phase 2: Degrading link 0");
    mock_stats.degrade(0, 200, 300); // High retrans and RTT

    run_pipeline_for_duration(&pipeline, 1).expect("Phase 2 failed");

    let phase2_count1: u64 = get_property(&counter1, "count").unwrap();
    let phase2_count2: u64 = get_property(&counter2, "count").unwrap();
    println!("Phase 2 - Counter 1: {}, Counter 2: {}", phase2_count1, phase2_count2);

    // Phase 3: Recover link 0
    println!("Phase 3: Recovering link 0");
    mock_stats.recover(0);

    run_pipeline_for_duration(&pipeline, 1).expect("Phase 3 failed");

    let phase3_count1: u64 = get_property(&counter1, "count").unwrap();
    let phase3_count2: u64 = get_property(&counter2, "count").unwrap();
    println!("Phase 3 - Counter 1: {}, Counter 2: {}", phase3_count1, phase3_count2);

    // Verify progression through all phases
    assert!(phase2_count1 > phase1_count1, "Should have continued sending in phase 2");
    assert!(phase3_count1 > phase2_count1, "Should have continued sending in phase 3");

    println!("✅ Single link degradation recovery test completed");
}

#[test]
fn test_multiple_recovery_cycles() {
    init_for_tests();

    println!("=== Multiple Recovery Cycles Test ===");

    let mock_stats = create_mock_stats(2);

    // Initial state: both links healthy
    mock_stats.tick(&[500, 500], &[5, 5], &[30, 30]);
    let initial_stats = mock_stats.property::<gst::Structure>("stats");
    println!("Initial stats: {}", initial_stats);

    // Cycle 1: Degrade session 0, then recover
    println!("Cycle 1: Link 0 degradation");
    mock_stats.degrade(0, 100, 200);
    let degraded_stats = mock_stats.property::<gst::Structure>("stats");
    
    let cycle1_retrans = degraded_stats.get::<u64>("session-0.sent-retransmitted-packets").unwrap();
    let cycle1_rtt = degraded_stats.get::<f64>("session-0.round-trip-time").unwrap();
    println!("After degradation - Retrans: {}, RTT: {}", cycle1_retrans, cycle1_rtt);

    mock_stats.recover(0);
    let recovered_stats = mock_stats.property::<gst::Structure>("stats");
    
    let cycle1_recovered_retrans = recovered_stats.get::<u64>("session-0.sent-retransmitted-packets").unwrap();
    let cycle1_recovered_rtt = recovered_stats.get::<f64>("session-0.round-trip-time").unwrap();
    println!("After recovery - Retrans: {}, RTT: {}", cycle1_recovered_retrans, cycle1_recovered_rtt);

    assert!(cycle1_recovered_retrans < cycle1_retrans, "Should recover from degradation");
    assert!(cycle1_recovered_rtt < cycle1_rtt, "RTT should improve");

    // Cycle 2: Degrade session 1, then recover
    println!("Cycle 2: Link 1 degradation");
    mock_stats.degrade(1, 150, 250);
    mock_stats.recover(1);

    let final_stats = mock_stats.property::<gst::Structure>("stats");
    println!("Final stats: {}", final_stats);

    let final_retrans_0 = final_stats.get::<u64>("session-0.sent-retransmitted-packets").unwrap();
    let final_retrans_1 = final_stats.get::<u64>("session-1.sent-retransmitted-packets").unwrap();

    // Both sessions should have reasonable stats after multiple cycles
    assert!(final_retrans_0 < 200, "Session 0 should have reasonable retrans count");
    assert!(final_retrans_1 < 200, "Session 1 should have reasonable retrans count");

    println!("✅ Multiple recovery cycles test completed");
}

#[test]
fn test_dispatcher_recovery_integration() {
    init_for_tests();

    println!("=== Dispatcher Recovery Integration Test ===");

    // Create a more complex pipeline with recovery simulation
    let source = create_test_source();
    let dispatcher = gst::ElementFactory::make("ristdispatcher")
        .property("rebalance-interval-ms", 50u64) // Very fast rebalancing
        .property("strategy", "ewma")
        .property("auto-balance", true)
        .build()
        .expect("Failed to create ristdispatcher");

    let mock_stats = create_mock_stats(3);
    let counter1 = create_counter_sink();
    let counter2 = create_counter_sink();
    let counter3 = create_counter_sink();

    test_pipeline!(pipeline, &source, &dispatcher, &counter1, &counter2, &counter3);

    // Set up three output paths
    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_2 = dispatcher.request_pad_simple("src_%u").unwrap();

    source.link(&dispatcher).expect("Failed to link source");
    src_0.link(&counter1.static_pad("sink").unwrap()).expect("Failed to link src_0");
    src_1.link(&counter2.static_pad("sink").unwrap()).expect("Failed to link src_1");
    src_2.link(&counter3.static_pad("sink").unwrap()).expect("Failed to link src_2");

    // Simulate complex scenario: two links degrade, then recover at different times
    mock_stats.tick(&[100, 100, 100], &[2, 2, 2], &[20, 20, 20]);

    // Run initial phase
    run_pipeline_for_duration(&pipeline, 1).expect("Initial phase failed");

    // Degrade links 0 and 1
    mock_stats.degrade(0, 50, 150);
    mock_stats.degrade(1, 75, 200);

    run_pipeline_for_duration(&pipeline, 1).expect("Degradation phase failed");

    // Recover link 0 first
    mock_stats.recover(0);
    run_pipeline_for_duration(&pipeline, 1).expect("Partial recovery phase failed");

    // Then recover link 1
    mock_stats.recover(1);
    run_pipeline_for_duration(&pipeline, 1).expect("Full recovery phase failed");

    // Verify all counters received some traffic
    let final_count1: u64 = get_property(&counter1, "count").unwrap();
    let final_count2: u64 = get_property(&counter2, "count").unwrap();
    let final_count3: u64 = get_property(&counter3, "count").unwrap();

    println!("Final counts - Counter 1: {}, Counter 2: {}, Counter 3: {}", 
             final_count1, final_count2, final_count3);

    assert!(final_count1 > 0, "Counter 1 should have received traffic");
    assert!(final_count2 > 0, "Counter 2 should have received traffic");
    assert!(final_count3 > 0, "Counter 3 should have received traffic");

    println!("✅ Dispatcher recovery integration test completed");
}

#[test]
fn test_graceful_degradation_behavior() {
    init_for_tests();

    println!("=== Graceful Degradation Behavior Test ===");

    let mock_stats = create_mock_stats(2);

    // Start with asymmetric but functional links
    mock_stats.tick(&[1000, 800], &[20, 15], &[35, 28]);

    let initial_stats = mock_stats.property::<gst::Structure>("stats");
    let session0_initial_retrans = initial_stats.get::<u64>("session-0.sent-retransmitted-packets").unwrap();
    let session1_initial_retrans = initial_stats.get::<u64>("session-1.sent-retransmitted-packets").unwrap();

    println!("Initial retrans - Session 0: {}, Session 1: {}", 
             session0_initial_retrans, session1_initial_retrans);

    // Gradually degrade one link while keeping the other stable
    mock_stats.degrade(0, 30, 50); // Moderate degradation
    let moderate_stats = mock_stats.property::<gst::Structure>("stats");
    let session0_moderate_retrans = moderate_stats.get::<u64>("session-0.sent-retransmitted-packets").unwrap();

    // Further degrade the same link
    mock_stats.degrade(0, 50, 100); // Severe degradation
    let severe_stats = mock_stats.property::<gst::Structure>("stats");
    let session0_severe_retrans = severe_stats.get::<u64>("session-0.sent-retransmitted-packets").unwrap();

    println!("Degradation progression - Moderate: {}, Severe: {}", 
             session0_moderate_retrans, session0_severe_retrans);

    assert!(session0_severe_retrans > session0_moderate_retrans, 
           "Progressive degradation should be reflected in stats");
    assert!(session0_moderate_retrans > session0_initial_retrans,
           "Moderate degradation should increase retrans");

    // Now test recovery brings it back to reasonable levels
    mock_stats.recover(0);
    let recovered_stats = mock_stats.property::<gst::Structure>("stats");
    let session0_recovered_retrans = recovered_stats.get::<u64>("session-0.sent-retransmitted-packets").unwrap();

    println!("After recovery: {}", session0_recovered_retrans);
    assert!(session0_recovered_retrans < session0_severe_retrans,
           "Recovery should improve the degraded link");

    println!("✅ Graceful degradation behavior test completed");
}
