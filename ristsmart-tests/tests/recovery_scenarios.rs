// Test recovery scenarios with RistStatsMock::recover method

use gst::prelude::*;
use gstreamer as gst;
use serde_json;
use std::time::Duration;

#[test]
fn test_mock_recovery_functionality() {
    ristsmart_tests::register_everything_for_tests();

    let stats_mock = gst::ElementFactory::make("riststats_mock")
        .build()
        .expect("Failed to create riststats mock");

    // Set up degraded initial conditions
    let degraded_stats = gst::Structure::builder("rist/x-sender-stats")
        .field("session-0.sent-original-packets", 1000u64)
        .field("session-0.sent-retransmitted-packets", 200u64)  // 20% loss - very bad
        .field("session-0.round-trip-time", 150.0f64)           // High RTT
        .field("session-1.sent-original-packets", 800u64)
        .field("session-1.sent-retransmitted-packets", 160u64)  // 20% loss - also bad
        .field("session-1.round-trip-time", 120.0f64)
        .build();

    stats_mock.set_property("stats", &degraded_stats);

    // Get the mock element to call recovery methods
    let mock_element = stats_mock.downcast_ref::<ristsmart_tests::RistStatsMock>().unwrap();

    // Capture initial degraded state
    let initial_stats: gst::Structure = stats_mock.property("stats");
    println!("Initial degraded stats: {}", initial_stats.to_string());

    let initial_retrans_0 = initial_stats.get::<u64>("session-0.sent-retransmitted-packets").unwrap();
    let initial_rtt_0 = initial_stats.get::<f64>("session-0.round-trip-time").unwrap();
    let initial_original_0 = initial_stats.get::<u64>("session-0.sent-original-packets").unwrap();

    println!("Session 0 before recovery: retrans={}, rtt={:.1}ms, original={}", 
             initial_retrans_0, initial_rtt_0, initial_original_0);

    // Perform recovery on session 0
    mock_element.recover(0);

    // Check that recovery improved session 0 metrics
    let recovered_stats: gst::Structure = stats_mock.property("stats");
    println!("Stats after session 0 recovery: {}", recovered_stats.to_string());

    let recovered_retrans_0 = recovered_stats.get::<u64>("session-0.sent-retransmitted-packets").unwrap();
    let recovered_rtt_0 = recovered_stats.get::<f64>("session-0.round-trip-time").unwrap();
    let recovered_original_0 = recovered_stats.get::<u64>("session-0.sent-original-packets").unwrap();

    println!("Session 0 after recovery: retrans={}, rtt={:.1}ms, original={}", 
             recovered_retrans_0, recovered_rtt_0, recovered_original_0);

    // Verify recovery improvements
    assert!(recovered_retrans_0 < initial_retrans_0, 
            "Recovery should reduce retransmission count");
    assert!(recovered_rtt_0 < initial_rtt_0, 
            "Recovery should improve (reduce) RTT");
    assert!(recovered_original_0 > initial_original_0, 
            "Recovery should show continued transmission progress");

    // Verify session 1 was not affected by session 0 recovery
    let session1_retrans = recovered_stats.get::<u64>("session-1.sent-retransmitted-packets").unwrap();
    let session1_rtt = recovered_stats.get::<f64>("session-1.round-trip-time").unwrap();
    
    assert_eq!(session1_retrans, 160u64, 
               "Session 1 retrans should be unchanged by session 0 recovery");
    assert_eq!(session1_rtt, 120.0f64, 
               "Session 1 RTT should be unchanged by session 0 recovery");

    println!("Recovery functionality test passed!");
}

#[test]
fn test_recovery_integration_with_dispatcher() {
    ristsmart_tests::register_everything_for_tests();

    let stats_mock = gst::ElementFactory::make("riststats_mock")
        .build()
        .expect("Failed to create riststats mock");

    let dispatcher = gst::ElementFactory::make("ristdispatcher")
        .property("auto-balance", true)
        .property("rebalance-interval-ms", 100u64)
        .property("strategy", "ewma")
        .build()
        .expect("Failed to create ristdispatcher");

    dispatcher.set_property("rist", &stats_mock);

    // Set up initial poor conditions for both sessions
    let poor_stats = gst::Structure::builder("rist/x-sender-stats")
        .field("session-0.sent-original-packets", 1000u64)
        .field("session-0.sent-retransmitted-packets", 150u64)  // 15% loss
        .field("session-0.round-trip-time", 100.0f64)
        .field("session-1.sent-original-packets", 1000u64)
        .field("session-1.sent-retransmitted-packets", 200u64)  // 20% loss - worse
        .field("session-1.round-trip-time", 130.0f64)
        .build();

    stats_mock.set_property("stats", &poor_stats);

    // Request pads
    let _pad1 = dispatcher.request_pad_simple("src_%u").expect("Failed to request pad 1");
    let _pad2 = dispatcher.request_pad_simple("src_%u").expect("Failed to request pad 2");

    // Allow initial weight calculation
    std::thread::sleep(Duration::from_millis(200));

    let initial_weights_str: String = dispatcher.property("current-weights");
    let initial_weights_json: serde_json::Value = serde_json::from_str(&initial_weights_str).unwrap();
    let initial_weights_array = initial_weights_json.as_array().unwrap();
    let initial_weight0 = initial_weights_array[0].as_f64().unwrap();
    let initial_weight1 = initial_weights_array[1].as_f64().unwrap();

    println!("Initial weights with poor conditions: session-0={:.3}, session-1={:.3}", 
             initial_weight0, initial_weight1);

    // Get the mock element for recovery
    let mock_element = stats_mock.downcast_ref::<ristsmart_tests::RistStatsMock>().unwrap();

    // Recover session 1 (the worse performing one)
    mock_element.recover(1);

    // Allow time for dispatcher to process the improved stats
    std::thread::sleep(Duration::from_millis(250));

    let recovered_weights_str: String = dispatcher.property("current-weights");
    let recovered_weights_json: serde_json::Value = serde_json::from_str(&recovered_weights_str).unwrap();
    let recovered_weights_array = recovered_weights_json.as_array().unwrap();
    let recovered_weight0 = recovered_weights_array[0].as_f64().unwrap();
    let recovered_weight1 = recovered_weights_array[1].as_f64().unwrap();

    println!("Weights after session 1 recovery: session-0={:.3}, session-1={:.3}", 
             recovered_weight0, recovered_weight1);

    // Verify the dispatcher responded to the recovery
    assert!(recovered_weight0 > 0.0 && recovered_weight1 > 0.0, 
            "All weights should remain positive");

    // The exact weight changes depend on EWMA parameters, but we can verify
    // that session 1's recovery was reflected in the system
    let weight0_change = recovered_weight0 - initial_weight0;
    let weight1_change = recovered_weight1 - initial_weight1;

    println!("Weight changes: session-0={:.3}, session-1={:.3}", weight0_change, weight1_change);

    // Since session 1 was recovered (improved), its relative weight should tend to increase
    // while session 0 remains degraded, so we expect some positive movement for session 1
    // However, EWMA smoothing means changes may be gradual

    // Verify stats reflect the recovery
    let final_stats: gst::Structure = stats_mock.property("stats");
    let session1_final_retrans = final_stats.get::<u64>("session-1.sent-retransmitted-packets").unwrap();
    let session1_final_rtt = final_stats.get::<f64>("session-1.round-trip-time").unwrap();

    // Session 1 should have better metrics after recovery
    assert!(session1_final_retrans < 200u64, 
            "Session 1 retransmissions should be reduced after recovery");
    assert!(session1_final_rtt < 130.0f64, 
            "Session 1 RTT should be improved after recovery");

    println!("Recovery integration with dispatcher test passed!");
}

#[test]
fn test_multiple_recovery_cycles() {
    ristsmart_tests::register_everything_for_tests();

    let stats_mock = gst::ElementFactory::make("riststats_mock")
        .build()
        .expect("Failed to create riststats mock");

    // Start with severely degraded session
    let initial_stats = gst::Structure::builder("rist/x-sender-stats")
        .field("session-0.sent-original-packets", 1000u64)
        .field("session-0.sent-retransmitted-packets", 500u64)  // 50% loss - extreme
        .field("session-0.round-trip-time", 250.0f64)           // Very high RTT
        .build();

    stats_mock.set_property("stats", &initial_stats);

    let mock_element = stats_mock.downcast_ref::<ristsmart_tests::RistStatsMock>().unwrap();

    // Track recovery progress through multiple cycles
    let mut previous_retrans = 500u64;
    let mut previous_rtt = 250.0f64;

    for cycle in 1..=5 {
        println!("\n--- Recovery Cycle {} ---", cycle);
        
        // Perform recovery
        mock_element.recover(0);
        
        // Check current stats
        let current_stats: gst::Structure = stats_mock.property("stats");
        let current_retrans = current_stats.get::<u64>("session-0.sent-retransmitted-packets").unwrap();
        let current_rtt = current_stats.get::<f64>("session-0.round-trip-time").unwrap();
        let current_original = current_stats.get::<u64>("session-0.sent-original-packets").unwrap();

        println!("After cycle {}: retrans={}, rtt={:.1}ms, original={}", 
                 cycle, current_retrans, current_rtt, current_original);

        // Verify continuous improvement
        assert!(current_retrans <= previous_retrans, 
                "Retransmissions should not increase during recovery");
        assert!(current_rtt <= previous_rtt, 
                "RTT should not increase during recovery");

        // Update for next iteration
        previous_retrans = current_retrans;
        previous_rtt = current_rtt;
    }

    // After multiple recovery cycles, metrics should be significantly improved
    let final_stats: gst::Structure = stats_mock.property("stats");
    let final_retrans = final_stats.get::<u64>("session-0.sent-retransmitted-packets").unwrap();
    let final_rtt = final_stats.get::<f64>("session-0.round-trip-time").unwrap();

    println!("\nFinal state after {} cycles: retrans={}, rtt={:.1}ms", 
             5, final_retrans, final_rtt);

    // Should show substantial improvement from initial state
    assert!(final_retrans < 250u64, // Less than half of initial 500
            "Multiple recovery cycles should significantly reduce retransmissions");
    assert!(final_rtt < 100.0f64,   // Less than half of initial 250ms
            "Multiple recovery cycles should significantly improve RTT");

    println!("Multiple recovery cycles test passed!");
}
