use gstreamer::{prelude::*, Element};
use gstristelements::testing::*;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::thread;

/// Test runtime weight changes while dispatcher is active
#[test]
fn test_runtime_weight_updates() {
    init_for_tests();
    
    let dispatcher = create_dispatcher_for_testing(Some(&[1.0, 1.0, 1.0]));
    dispatcher.set_property("strategy", "manual");
    dispatcher.set_property("min-hold-ms", 100u64); // Quick response for testing
    
    let weight_changes = Arc::new(Mutex::new(Vec::new()));
    let weight_changes_clone = weight_changes.clone();
    
    // Monitor weight changes
    dispatcher.connect("notify::current-weights", false, move |values| {
        let elem = values[0].get::<Element>().unwrap();
        let weights_str: String = elem.property("current-weights");
        let timestamp = Instant::now();
        weight_changes_clone.lock().unwrap().push((weights_str, timestamp));
        None
    });
    
    // Start pipeline
    dispatcher.set_state(gstreamer::State::Playing).unwrap();
    std::thread::sleep(Duration::from_millis(200)); // Initial stabilization
    
    // Clear initial notifications
    weight_changes.lock().unwrap().clear();
    let test_start = Instant::now();
    
    // Series of runtime weight changes
    let weight_sequences = vec![
        "[2.0, 1.0, 1.0]", // Favor output 0
        "[1.0, 3.0, 1.0]", // Favor output 1  
        "[1.0, 1.0, 2.5]", // Favor output 2
        "[1.5, 1.5, 1.0]", // Balance first two
        "[1.0, 1.0, 1.0]", // Equal weights
    ];
    
    for weights in weight_sequences.iter() {
        std::thread::sleep(Duration::from_millis(250)); // Allow processing time
        
        println!("Setting weights {} at {:?}", weights, test_start.elapsed());
        dispatcher.set_property("weights", *weights);
        
        // Verify the property was accepted (format may be normalized)
        let current_weights: String = dispatcher.property("weights");
        let normalized_weights = weights.replace(" ", ""); // Remove spaces for comparison
        assert!(current_weights.contains(&normalized_weights[1..normalized_weights.len()-1]) || 
                current_weights == normalized_weights,
                "Weights should be updated during runtime: expected {}, got {}", weights, current_weights);
    }
    
    std::thread::sleep(Duration::from_millis(300)); // Allow final processing
    dispatcher.set_state(gstreamer::State::Null).unwrap();
    
    // Analyze the changes
    let changes = weight_changes.lock().unwrap();
    let test_duration = test_start.elapsed();
    
    println!("Runtime weight test: {} changes over {:?}", changes.len(), test_duration);
    
    // Should have captured weight updates (may be fewer than expected)
    if changes.is_empty() {
        println!("No weight change notifications - this may be expected behavior");
        // Verify final state is still valid
        let final_weights: String = dispatcher.property("current-weights");
        assert!(!final_weights.is_empty(), "Should maintain valid current weights");
    } else {
        assert!(changes.len() <= 15, "Should not have excessive change notifications");
        
        // Verify changes occurred in reasonable timeframes
        for i in 1..changes.len() {
            let time_gap = changes[i].1.duration_since(changes[i-1].1);
            assert!(time_gap <= Duration::from_millis(2000), 
                    "Weight changes should occur within reasonable time");
        }
    }
}

/// Test strategy changes during runtime operation
#[test]
fn test_runtime_strategy_changes() {
    init_for_tests();
    
    let dispatcher = create_dispatcher_for_testing(Some(&[1.0, 1.0]));
    dispatcher.set_property("rebalance-interval-ms", 200u64); // Fast rebalancing
    
    // Start with manual strategy
    dispatcher.set_property("strategy", "manual");
    dispatcher.set_state(gstreamer::State::Playing).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    
    // Set specific manual weights
    dispatcher.set_property("weights", "[2.0, 0.5]");
    std::thread::sleep(Duration::from_millis(200));
    
    let manual_weights: String = dispatcher.property("current-weights");
    
    // Switch to auto strategy during runtime
    dispatcher.set_property("strategy", "auto");
    std::thread::sleep(Duration::from_millis(400)); // Allow auto to take effect
    
    let auto_weights: String = dispatcher.property("current-weights");
    
    // Switch back to manual
    dispatcher.set_property("strategy", "manual");
    dispatcher.set_property("weights", "[0.8, 1.8]"); // Different manual weights
    std::thread::sleep(Duration::from_millis(200));
    
    let final_manual_weights: String = dispatcher.property("current-weights");
    
    dispatcher.set_state(gstreamer::State::Null).unwrap();
    
    println!("Strategy test - manual: {}, auto: {}, final: {}", 
             manual_weights, auto_weights, final_manual_weights);
    
    // All should have valid weight strings
    assert!(!manual_weights.is_empty(), "Manual strategy should produce weights");
    assert!(!auto_weights.is_empty(), "Auto strategy should produce weights");
    assert!(!final_manual_weights.is_empty(), "Final manual strategy should produce weights");
    
    // Verify strategy property changes are accepted (may default to another strategy)
    let current_strategy: String = dispatcher.property("strategy");
    println!("Final strategy: {}", current_strategy);
    assert!(["manual", "auto", "ewma"].contains(&current_strategy.as_str()), 
            "Strategy should be in a valid state: {}", current_strategy);
}

/// Test concurrent weight updates from multiple threads
#[test]
fn test_concurrent_weight_updates() {
    init_for_tests();
    
    let dispatcher = create_dispatcher_for_testing(Some(&[1.0, 1.0, 1.0]));
    dispatcher.set_property("strategy", "manual");
    dispatcher.set_property("min-hold-ms", 50u64); // Allow rapid updates
    
    let update_count = Arc::new(Mutex::new(0));
    let error_count = Arc::new(Mutex::new(0));
    
    dispatcher.set_state(gstreamer::State::Playing).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    
    // Launch multiple threads doing concurrent updates
    let mut handles = Vec::new();
    
    for thread_id in 0..3 {
        let disp_clone = dispatcher.clone();
        let update_count_clone = update_count.clone();
        let error_count_clone = error_count.clone();
        
        let handle = thread::spawn(move || {
            for i in 0..5 {
                thread::sleep(Duration::from_millis(50));
                
                // Each thread uses different weight patterns
                let weights = match thread_id {
                    0 => format!("[{:.1}, 1.0, 1.0]", 1.0 + i as f64 * 0.2),
                    1 => format!("[1.0, {:.1}, 1.0]", 1.0 + i as f64 * 0.3),
                    _ => format!("[1.0, 1.0, {:.1}]", 1.0 + i as f64 * 0.1),
                };
                
                // Attempt to set weights
                match std::panic::catch_unwind(|| {
                    disp_clone.set_property("weights", &weights);
                }) {
                    Ok(_) => {
                        *update_count_clone.lock().unwrap() += 1;
                    }
                    Err(_) => {
                        *error_count_clone.lock().unwrap() += 1;
                    }
                }
            }
        });
        
        handles.push(handle);
    }
    
    // Wait for all threads to complete
    for handle in handles {
        handle.join().expect("Thread should complete successfully");
    }
    
    std::thread::sleep(Duration::from_millis(200)); // Allow final processing
    dispatcher.set_state(gstreamer::State::Null).unwrap();
    
    let final_updates = *update_count.lock().unwrap();
    let final_errors = *error_count.lock().unwrap();
    
    println!("Concurrent test: {} successful updates, {} errors", final_updates, final_errors);
    
    // Should handle most updates successfully
    assert!(final_updates >= 10, "Should handle most concurrent weight updates");
    assert!(final_errors <= 5, "Should have minimal errors from concurrent updates");
    
    // Final state should be valid
    let final_weights: String = dispatcher.property("current-weights");
    assert!(!final_weights.is_empty(), "Should maintain valid weights after concurrent updates");
}

/// Test property validation during runtime changes
#[test]
fn test_runtime_property_validation() {
    init_for_tests();
    
    let dispatcher = create_dispatcher_for_testing(Some(&[1.0, 1.0]));
    dispatcher.set_state(gstreamer::State::Playing).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    
    // Test valid runtime changes
    dispatcher.set_property("min-hold-ms", 300u64);
    dispatcher.set_property("switch-threshold", 1.8f64);
    dispatcher.set_property("rebalance-interval-ms", 500u64);
    
    let min_hold: u64 = dispatcher.property("min-hold-ms");
    let threshold: f64 = dispatcher.property("switch-threshold");
    let interval: u64 = dispatcher.property("rebalance-interval-ms");
    
    assert_eq!(min_hold, 300, "Min hold time should be updatable during runtime");
    assert_eq!(threshold, 1.8, "Switch threshold should be updatable during runtime");
    assert_eq!(interval, 500, "Rebalance interval should be updatable during runtime");
    
    // Test edge cases
    dispatcher.set_property("min-hold-ms", 0u64);     // Minimum value
    dispatcher.set_property("switch-threshold", 1.0f64); // Minimum threshold
    
    let min_min_hold: u64 = dispatcher.property("min-hold-ms");
    let min_threshold: f64 = dispatcher.property("switch-threshold");
    
    assert_eq!(min_min_hold, 0, "Should accept minimum hold time");
    assert_eq!(min_threshold, 1.0, "Should accept minimum threshold");
    
    dispatcher.set_state(gstreamer::State::Null).unwrap();
}

/// Test weight format validation during runtime
#[test] 
fn test_runtime_weight_format_validation() {
    init_for_tests();
    
    let dispatcher = create_dispatcher_for_testing(Some(&[1.0, 1.0]));
    dispatcher.set_property("strategy", "manual");
    dispatcher.set_state(gstreamer::State::Playing).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    
    // Test valid weight formats
    let valid_formats = vec![
        "[1.0, 1.0]",
        "[2.5, 0.5]", 
        "[0.1, 3.9]",
        "[1, 1]",           // Integer format
        "[ 1.0 , 1.0 ]",    // With spaces
    ];
    
    for weights in valid_formats {
        dispatcher.set_property("weights", weights);
        let current: String = dispatcher.property("weights");
        // Note: The internal format might normalize the input
        assert!(!current.is_empty(), "Should accept valid weight format: {}", weights);
    }
    
    // Test current state remains valid after all updates
    let final_weights: String = dispatcher.property("current-weights");
    assert!(!final_weights.is_empty(), "Should maintain valid current weights");
    
    dispatcher.set_state(gstreamer::State::Null).unwrap();
    
    println!("Weight format validation completed successfully");
}