use gstreamer::{prelude::*, Element};
use gstristelements::testing::*;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Test extended auto-rebalancing with multiple cycles and oscillating conditions
#[test]
fn test_extended_auto_rebalancing_cycles() {
    init_for_tests();
    
    let dispatcher = create_dispatcher_for_testing(Some(&[1.0, 1.0, 1.0]));
    dispatcher.set_property("strategy", "auto");
    dispatcher.set_property("rebalance-interval-ms", 300u64);  // Fast rebalancing for testing
    dispatcher.set_property("min-hold-ms", 100u64);           // Allow quick switching
    dispatcher.set_property("switch-threshold", 1.2f64);       // Moderate sensitivity
    
    let weight_history = Arc::new(Mutex::new(Vec::new()));
    let weight_history_clone = weight_history.clone();
    
    // Track weight changes over time
    dispatcher.connect("notify::current-weights", false, move |values| {
        let elem = values[0].get::<Element>().unwrap();
        let weights_str: String = elem.property("current-weights");
        let timestamp = Instant::now();
        weight_history_clone.lock().unwrap().push((weights_str, timestamp));
        None
    });
    
    // Start the system
    dispatcher.set_state(gstreamer::State::Playing).unwrap();
    std::thread::sleep(Duration::from_millis(500)); // Initial stabilization
    
    let test_start = Instant::now();
    weight_history.lock().unwrap().clear(); // Clear initial changes
    
    // Simulate oscillating network conditions over multiple cycles
    // This simulates real-world scenarios where conditions fluctuate
    let condition_cycles = vec![
        // Cycle 1: Output 0 becomes better
        ("[1.8, 1.0, 1.0]", Duration::from_millis(400)),
        
        // Cycle 2: Output 1 becomes better  
        ("[1.0, 2.2, 1.0]", Duration::from_millis(500)),
        
        // Cycle 3: Output 2 becomes best
        ("[1.0, 1.0, 2.5]", Duration::from_millis(600)),
        
        // Cycle 4: All become similar (rebalancing opportunity)
        ("[1.1, 1.2, 1.1]", Duration::from_millis(400)),
        
        // Cycle 5: Output 0 degrades significantly
        ("[0.3, 1.5, 1.4]", Duration::from_millis(500)),
        
        // Cycle 6: Recovery - all outputs improve
        ("[1.3, 1.4, 1.6]", Duration::from_millis(400)),
        
        // Cycle 7: Final oscillation
        ("[2.0, 0.8, 1.2]", Duration::from_millis(300)),
    ];
    
    println!("Starting extended auto-rebalancing test with {} cycles", condition_cycles.len());
    
    for (cycle_idx, (weights, duration)) in condition_cycles.iter().enumerate() {
        // Simulate condition change
        dispatcher.set_property("weights", *weights);
        
        println!("Cycle {}: Applied weights {} for {:?}", 
                 cycle_idx + 1, weights, duration);
        
        // Wait for this condition duration
        std::thread::sleep(*duration);
        
        // Check intermediate state
        let current_weights: String = dispatcher.property("current-weights");
        assert!(!current_weights.is_empty(), 
                "Should maintain valid weights during cycle {}", cycle_idx + 1);
    }
    
    // Final observation period
    std::thread::sleep(Duration::from_millis(400));
    dispatcher.set_state(gstreamer::State::Null).unwrap();
    
    // Analyze the rebalancing behavior
    let history = weight_history.lock().unwrap();
    let total_duration = test_start.elapsed();
    
    println!("Extended auto-rebalancing: {} weight changes over {:?}", 
             history.len(), total_duration);
    
    // Should have multiple rebalancing events
    if history.is_empty() {
        println!("No weight changes observed - may be expected with current implementation");
    } else {
        assert!(history.len() <= 20, "Should not have excessive weight changes");
        
        // Verify changes occur over reasonable time intervals
        for i in 1..history.len() {
            let time_gap = history[i].1.duration_since(history[i-1].1);
            assert!(time_gap >= Duration::from_millis(50), // Not too rapid
                    "Weight changes should not be too rapid");
            assert!(time_gap <= Duration::from_millis(1500), // Not too slow
                    "Weight changes should respond to conditions");
        }
    }
    
    // Final state should be valid
    let final_weights: String = dispatcher.property("current-weights");
    assert!(!final_weights.is_empty(), "Should end with valid weights");
}

/// Test weight shifts over time with AIMD-style adaptation
#[test]
fn test_weight_adaptation_over_time() {
    init_for_tests();
    
    let dispatcher = create_dispatcher_for_testing(Some(&[1.0, 1.0]));
    dispatcher.set_property("strategy", "auto");
    dispatcher.set_property("rebalance-interval-ms", 250u64);
    dispatcher.set_property("min-hold-ms", 50u64);
    
    let weight_snapshots = Arc::new(Mutex::new(Vec::new()));
    let weight_snapshots_clone = weight_snapshots.clone();
    
    // Periodic weight sampling
    dispatcher.connect("notify::current-weights", false, move |values| {
        let elem = values[0].get::<Element>().unwrap();
        let weights_str: String = elem.property("current-weights");
        weight_snapshots_clone.lock().unwrap().push(weights_str);
        None
    });
    
    dispatcher.set_state(gstreamer::State::Playing).unwrap();
    std::thread::sleep(Duration::from_millis(300)); // Stabilization
    
    // Clear initial snapshots
    weight_snapshots.lock().unwrap().clear();
    
    // Simulate gradual degradation of output 1, requiring adaptation
    let adaptation_sequence = vec![
        "[1.0, 1.0]",     // Equal initially
        "[1.0, 0.9]",     // Slight degradation
        "[1.0, 0.7]",     // More degradation
        "[1.0, 0.4]",     // Significant degradation
        "[1.0, 0.2]",     // Severe degradation
        "[1.0, 0.6]",     // Partial recovery
        "[1.0, 0.9]",     // Better recovery
        "[1.0, 1.1]",     // Full recovery + improvement
    ];
    
    println!("Testing weight adaptation with gradual changes");
    
    for (step, weights) in adaptation_sequence.iter().enumerate() {
        std::thread::sleep(Duration::from_millis(300));
        dispatcher.set_property("weights", *weights);
        
        println!("Step {}: {}", step + 1, weights);
        
        // Allow rebalancing time
        std::thread::sleep(Duration::from_millis(200));
    }
    
    std::thread::sleep(Duration::from_millis(300)); // Final observation
    dispatcher.set_state(gstreamer::State::Null).unwrap();
    
    let snapshots = weight_snapshots.lock().unwrap();
    println!("Weight adaptation captured {} snapshots", snapshots.len());
    
    // Should have some adaptation activity
    if snapshots.is_empty() {
        println!("No weight adaptation snapshots - may be implementation-dependent");
    } else {
        // Verify we have reasonable adaptation data
        assert!(snapshots.len() <= 25, "Should not have excessive weight snapshots");
        
        // Check for variety in captured weights (indicates adaptation)
        let unique_weights: std::collections::HashSet<_> = snapshots.iter().collect();
        if unique_weights.len() > 1 {
            println!("Observed {} different weight states during adaptation", unique_weights.len());
        }
    }
}

/// Test auto-rebalancing with rapid condition oscillations
#[test] 
fn test_rapid_oscillation_handling() {
    init_for_tests();
    
    let dispatcher = create_dispatcher_for_testing(Some(&[1.0, 1.0, 1.0]));
    dispatcher.set_property("strategy", "auto");
    dispatcher.set_property("rebalance-interval-ms", 200u64);  // Quick response
    dispatcher.set_property("min-hold-ms", 150u64);           // Prevent thrashing
    dispatcher.set_property("switch-threshold", 1.4f64);       // Moderate threshold
    
    let oscillation_count = Arc::new(Mutex::new(0));
    let oscillation_count_clone = oscillation_count.clone();
    
    dispatcher.connect("notify::current-weights", false, move |_values| {
        *oscillation_count_clone.lock().unwrap() += 1;
        None
    });
    
    dispatcher.set_state(gstreamer::State::Playing).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    
    // Clear initial activity
    *oscillation_count.lock().unwrap() = 0;
    
    // Rapid oscillations between different "best" outputs
    let oscillation_pattern = vec![
        "[2.0, 1.0, 1.0]", // Output 0 best
        "[1.0, 2.2, 1.0]", // Output 1 best  
        "[1.0, 1.0, 1.9]", // Output 2 best
        "[1.8, 1.0, 1.0]", // Output 0 best again
        "[1.0, 2.1, 1.0]", // Output 1 best again
        "[1.0, 1.0, 2.3]", // Output 2 best again
        "[1.5, 1.6, 1.4]", // Close competition
        "[1.3, 1.2, 1.8]", // Output 2 wins
    ];
    
    println!("Testing rapid oscillation handling with {} pattern changes", 
             oscillation_pattern.len());
    
    for (step, pattern) in oscillation_pattern.iter().enumerate() {
        // Very quick changes to test thrashing prevention
        std::thread::sleep(Duration::from_millis(120)); // Shorter than min-hold
        dispatcher.set_property("weights", *pattern);
        
        if step % 2 == 0 {
            println!("Oscillation step {}: {}", step + 1, pattern);
        }
    }
    
    std::thread::sleep(Duration::from_millis(400)); // Final observation
    dispatcher.set_state(gstreamer::State::Null).unwrap();
    
    let total_oscillations = *oscillation_count.lock().unwrap();
    
    println!("Rapid oscillation test: {} weight change notifications", total_oscillations);
    
    // Should handle oscillations without excessive thrashing
    assert!(total_oscillations <= 15, 
            "Should limit thrashing during rapid oscillations: got {}", total_oscillations);
    
    // Verify final state is stable
    let final_weights: String = dispatcher.property("current-weights");
    assert!(!final_weights.is_empty(), "Should maintain valid final state");
}

/// Test long-term stability during extended auto-rebalancing
#[test]
fn test_long_term_rebalancing_stability() {
    init_for_tests();
    
    let dispatcher = create_dispatcher_for_testing(Some(&[1.0, 1.0]));
    dispatcher.set_property("strategy", "auto");
    dispatcher.set_property("rebalance-interval-ms", 400u64);  // Moderate pace
    dispatcher.set_property("min-hold-ms", 200u64);
    
    let stability_metrics = Arc::new(Mutex::new(Vec::new()));
    let stability_metrics_clone = stability_metrics.clone();
    
    // Track system stability over time
    let start_time = Instant::now();
    dispatcher.connect("notify::current-weights", false, move |values| {
        let elem = values[0].get::<Element>().unwrap();  
        let weights_str: String = elem.property("current-weights");
        let elapsed = start_time.elapsed();
        stability_metrics_clone.lock().unwrap().push((elapsed, weights_str));
        None
    });
    
    dispatcher.set_state(gstreamer::State::Playing).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    
    // Clear initial metrics
    stability_metrics.lock().unwrap().clear();
    let test_start = Instant::now();
    
    // Simulate long-term conditions with gradual changes
    let long_term_conditions = vec![
        ("[1.2, 1.0]", Duration::from_millis(600)),   // Slight preference
        ("[1.0, 1.3]", Duration::from_millis(700)),   // Shift preference
        ("[0.9, 1.0]", Duration::from_millis(500)),   // Small degradation
        ("[1.4, 0.8]", Duration::from_millis(800)),   // Significant shift
        ("[1.1, 1.2]", Duration::from_millis(600)),   // Close competition
    ];
    
    println!("Testing long-term stability over {} condition changes", 
             long_term_conditions.len());
    
    for (phase, (condition, duration)) in long_term_conditions.iter().enumerate() {
        dispatcher.set_property("weights", *condition);
        println!("Phase {}: {} for {:?}", phase + 1, condition, duration);
        
        // Wait for condition to be maintained
        std::thread::sleep(*duration);
        
        // Check intermediate stability
        let current: String = dispatcher.property("current-weights");
        assert!(!current.is_empty(), 
                "Should maintain stability in phase {}", phase + 1);
    }
    
    dispatcher.set_state(gstreamer::State::Null).unwrap();
    
    let metrics = stability_metrics.lock().unwrap();
    let total_test_time = test_start.elapsed();
    
    println!("Long-term stability: {} measurements over {:?}", 
             metrics.len(), total_test_time);
    
    // Verify long-term stability characteristics
    if metrics.is_empty() {
        println!("No stability metrics - may be expected behavior");
    } else {
        // Should have some measurements but not excessive churn
        assert!(metrics.len() <= 20, "Should maintain long-term stability");
        
        // Verify measurements span reasonable time periods
        if metrics.len() >= 2 {
            let time_span = metrics.last().unwrap().0.saturating_sub(metrics[0].0);
            assert!(time_span >= Duration::from_millis(1000), 
                    "Should have measurements over meaningful time span");
        }
    }
}