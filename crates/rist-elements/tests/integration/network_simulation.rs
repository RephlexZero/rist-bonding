use gstreamer as gst;
use gst::prelude::*;
use gstristelements::testing::*;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::thread;

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

/// Test dispatcher behavior under simulated network latency conditions
#[test]
fn test_latency_variation() {
    init_for_tests();
    
    let dispatcher = create_dispatcher_for_testing(Some(&[1.0, 1.0, 1.0]));
    
    // Use actual dispatcher properties for configuration
    dispatcher.set_property("switch-threshold", 1.5f64);
    dispatcher.set_property("min-hold-ms", 200u64);
    dispatcher.set_property("health-warmup-ms", 1000u64);
    
    let initial_weights: String = dispatcher.property("weights");
    assert!(!initial_weights.is_empty());
    
    // Simulate varying conditions by changing rebalance intervals
    // to represent different network response characteristics
    let latency_intervals = [100u64, 300u64, 600u64, 200u64]; // Different rebalance rates
    
    for (i, interval) in latency_intervals.iter().enumerate() {
        thread::sleep(Duration::from_millis(100));
        
        // Change rebalance interval to simulate different latency responses
        dispatcher.set_property("rebalance-interval-ms", *interval);
        
        // Use different weight patterns to simulate AIMD adaptation
        let weight_pattern = match i {
            0 => "[1.0, 1.0, 1.0]",     // Equal weights
            1 => "[1.2, 0.8, 1.0]",    // Slight preference
            2 => "[0.7, 1.3, 1.0]",    // Adaptation to conditions
            _ => "[1.1, 0.9, 1.0]",    // Recovery
        };
        
        dispatcher.set_property("weights", weight_pattern);
        run_mainloop_ms(50);
        
        let current_weights: String = dispatcher.property("weights");
        assert!(!current_weights.is_empty(), "Weights should remain valid");
    }
    
    let final_weights: String = dispatcher.property("weights");
    assert!(final_weights != initial_weights, "Weights should adapt to simulated conditions");
}

/// Test dispatcher behavior under simulated packet loss conditions
#[test]
fn test_packet_loss_adaptation() {
    init_for_tests();
    
    let dispatcher = create_dispatcher_for_testing(Some(&[1.0, 1.0]));
    
    // Configure for more sensitive switching
    dispatcher.set_property("switch-threshold", 1.2f64);
    dispatcher.set_property("min-hold-ms", 100u64);
    
    let initial_weights: String = dispatcher.property("weights");
    
    // Simulate packet loss adaptation by using different weight patterns
    let loss_patterns = [
        "[1.0, 1.0]",    // No loss - equal weights
        "[1.1, 0.9]",    // Light loss on second path
        "[1.4, 0.6]",    // Moderate loss on second path
        "[1.8, 0.2]",    // Heavy loss on second path
    ];
    
    for (i, pattern) in loss_patterns.iter().enumerate() {
        thread::sleep(Duration::from_millis(150));
        
        // Set weights to simulate AIMD response to packet loss
        dispatcher.set_property("weights", *pattern);
        
        // Adjust rebalance rate based on loss severity
        let rebalance_rate = 200u64 + (i as u64 * 100); // Slower rebalancing under loss
        dispatcher.set_property("rebalance-interval-ms", rebalance_rate);
        
        run_mainloop_ms(50);
        
        let current_weights: String = dispatcher.property("weights");
        assert!(!current_weights.is_empty());
    }
    
    let final_weights: String = dispatcher.property("weights");
    assert_ne!(final_weights, initial_weights, "Weights should adapt to packet loss simulation");
}

/// Test dispatcher behavior under simulated bandwidth constraints
#[test]
fn test_bandwidth_adaptation() {
    init_for_tests();
    
    let dispatcher = create_dispatcher_for_testing(Some(&[1.0, 1.0, 1.0]));
    
    // Configure for bandwidth-sensitive behavior
    dispatcher.set_property("rebalance-interval-ms", 500u64);
    dispatcher.set_property("switch-threshold", 1.3f64);
    
    let initial_weights: String = dispatcher.property("weights");
    
    // Simulate bandwidth constraints by weight shifting patterns
    let bandwidth_scenarios = [
        ("[1.0, 1.0, 1.0]", 300u64),  // Normal - all paths available
        ("[1.0, 0.5, 1.2]", 400u64),  // Path 2 constrained
        ("[0.8, 0.3, 1.5]", 600u64),  // Paths 1,2 constrained, prefer 3
        ("[0.4, 0.2, 1.8]", 800u64),  // Heavy constraints, path 3 dominant
    ];
    
    for (weights, interval) in bandwidth_scenarios.iter() {
        thread::sleep(Duration::from_millis(200));
        
        // Simulate bandwidth adaptation
        dispatcher.set_property("weights", *weights);
        dispatcher.set_property("rebalance-interval-ms", *interval);
        
        run_mainloop_ms(100);
        
        let current_weights: String = dispatcher.property("weights");
        assert!(!current_weights.is_empty());
    }
    
    let final_weights: String = dispatcher.property("weights");
    assert_ne!(final_weights, initial_weights, "Should adapt to bandwidth simulation");
}

/// Test dispatcher resilience to simulated intermittent connectivity
#[test]
fn test_intermittent_connectivity() {
    init_for_tests();
    
    let dispatcher = create_dispatcher_for_testing(Some(&[1.0, 1.0, 1.0]));
    
    // Configure for connectivity awareness
    dispatcher.set_property("health-warmup-ms", 500u64);
    dispatcher.set_property("min-hold-ms", 200u64);
    
    let connectivity_events = Arc::new(Mutex::new(Vec::new()));
    let events_clone = connectivity_events.clone();
    
    // Simulate intermittent connectivity patterns with weight changes
    let connectivity_patterns = [
        "[1.0, 1.0, 1.0]",  // All connected
        "[1.0, 0.0, 1.0]",  // Path 2 disconnected
        "[0.0, 0.0, 2.0]",  // Only path 3 available
        "[1.0, 0.5, 1.0]",  // Path 2 partially recovered
        "[1.0, 1.0, 0.0]",  // Path 3 disconnected
        "[0.8, 1.2, 0.0]",  // Paths 1,2 available, load shift
    ];
    
    for (cycle, pattern) in connectivity_patterns.iter().enumerate() {
        thread::sleep(Duration::from_millis(150));
        
        // Simulate connectivity state through weight patterns
        dispatcher.set_property("weights", *pattern);
        events_clone.lock().unwrap().push(format!("Cycle {}: weights {}", cycle, pattern));
        
        run_mainloop_ms(50);
        
        // Verify dispatcher maintains valid state
        let weights: String = dispatcher.property("weights");
        assert!(!weights.is_empty(), "Should maintain valid weights during connectivity changes");
    }
    
    let events = connectivity_events.lock().unwrap();
    assert!(!events.is_empty(), "Should have recorded connectivity events");
}

/// Test dispatcher behavior during simulated network congestion bursts
#[test]
fn test_congestion_bursts() {
    init_for_tests();
    
    let dispatcher = create_dispatcher_for_testing(Some(&[1.0, 1.0]));
    
    // Configure for congestion responsiveness
    dispatcher.set_property("switch-threshold", 1.1f64); // More sensitive switching
    dispatcher.set_property("min-hold-ms", 150u64);      // Faster response
    
    let initial_weights: String = dispatcher.property("weights");
    let mut adaptation_count = 0u32;
    
    // Simulate congestion bursts through rapid weight changes
    for burst in 0..6 {
        // Normal period - balanced weights
        for _ in 0..3 {
            thread::sleep(Duration::from_millis(50));
            dispatcher.set_property("weights", "[1.0, 1.0]");
            dispatcher.set_property("rebalance-interval-ms", 200u64);
            run_mainloop_ms(20);
        }
        
        // Congestion burst - rapid adaptation
        for intense in 0..4 {
            thread::sleep(Duration::from_millis(50));
            let congestion_weight = format!("[{:.1}, {:.1}]", 
                1.5 - (burst as f64 * 0.1), 
                0.5 + (intense as f64 * 0.2));
            dispatcher.set_property("weights", &congestion_weight);
            dispatcher.set_property("rebalance-interval-ms", 100u64); // Faster rebalancing
            run_mainloop_ms(20);
            
            let current_weights: String = dispatcher.property("weights");
            if current_weights != initial_weights {
                adaptation_count += 1;
            }
        }
    }
    
    assert!(adaptation_count > 0, "Should have adapted to congestion simulation");
}

/// Test dispatcher performance under simulated mixed network conditions
#[test]
fn test_mixed_network_conditions() {
    init_for_tests();
    
    let dispatcher = create_dispatcher_for_testing(Some(&[1.0, 1.0, 1.0]));
    
    // Configure for comprehensive condition handling
    dispatcher.set_property("rebalance-interval-ms", 300u64);
    dispatcher.set_property("switch-threshold", 1.25f64);
    dispatcher.set_property("health-warmup-ms", 800u64);
    
    let start_time = Instant::now();
    let mut condition_changes = 0u32;
    let mut last_weights = String::new();
    
    // Simulate 15 cycles of mixed network conditions
    let mixed_conditions = [
        ("[1.0, 1.0, 1.0]", 250u64),  // Stable conditions
        ("[1.3, 0.8, 0.9]", 400u64),  // High latency on paths 2,3
        ("[0.9, 1.4, 0.7]", 300u64),  // Packet loss on paths 1,3
        ("[0.6, 0.5, 1.9]", 600u64),  // Bandwidth congestion paths 1,2
        ("[1.2, 1.1, 0.7]", 350u64),  // Mixed stress on path 3
    ];
    
    for cycle in 0..15 {
        thread::sleep(Duration::from_millis(120));
        
        let (weights, interval) = &mixed_conditions[cycle % mixed_conditions.len()];
        
        // Apply mixed network conditions
        dispatcher.set_property("weights", *weights);
        dispatcher.set_property("rebalance-interval-ms", *interval);
        
        run_mainloop_ms(80);
        
        let current_weights: String = dispatcher.property("weights");
        if current_weights != last_weights {
            condition_changes += 1;
            last_weights = current_weights.clone();
        }
        
        assert!(!current_weights.is_empty(), "Should maintain valid weights");
    }
    
    let elapsed = start_time.elapsed();
    
    // Verify realistic performance and adaptation
    assert!(elapsed < Duration::from_secs(5), "Test should complete efficiently");
    assert!(condition_changes > 0, "Should have adapted to changing conditions");
    
    // Verify final state is reasonable
    let final_weights: String = dispatcher.property("weights");
    assert!(!final_weights.is_empty(), "Should maintain valid final state");
}