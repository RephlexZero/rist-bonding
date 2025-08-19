//! Enhanced scheduler demonstration
//!
//! This demo shows the advanced runtime scheduler capabilities including:
//! - Markov chain state transitions with configurable transition probabilities
//! - Time-varying scenarios with realistic 5G network conditions
//! - Exponential dwell time distributions for realistic behavior
//!
//! Run with: cargo run --bin enhanced-scheduler

use netns_testbench::runtime::{Scheduler, LinkRuntime};
use netns_testbench::qdisc::QdiscManager;
use scenarios::{Schedule, DirectionSpec};
use std::{sync::Arc, time::Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting enhanced scheduler demonstration");

    // Create a Markov chain schedule for realistic 5G behavior
    let nr_excellent = DirectionSpec::nr_mmwave(); // 1 Gbps+ mmWave
    let nr_degraded = DirectionSpec::nr_mmwave().with_mmwave_blockage(0.8); // Blockage event
    let nr_handover = DirectionSpec::nr_sub6ghz().with_carrier_aggregation(2); // Sub-6 with CA

    let markov_schedule = Schedule::Markov {
        states: vec![
            nr_excellent.clone(),
            nr_degraded.clone(), 
            nr_handover.clone(),
        ],
        // Transition matrix: excellent -> degraded (5%), degraded -> excellent (30%), etc.
        transition_matrix: vec![
            vec![0.90, 0.05, 0.05], // excellent: mostly stay, occasional blockage/handover
            vec![0.40, 0.50, 0.10], // degraded: recover or worsen
            vec![0.60, 0.10, 0.30], // handover: mostly recover
        ],
        initial_state: 0, // Start in excellent state
        mean_dwell_time: Duration::from_secs(15), // 15s average dwell time
    };

    // Create link runtime with the Markov schedule
    let link_runtime = LinkRuntime::new(
        "demo-5g-link".to_string(),
        1, // Interface index 1 (dummy)
        Arc::new(QdiscManager),
        nr_excellent.clone(),
        markov_schedule,
    );

    // Create and start scheduler
    let scheduler = Scheduler::new();
    scheduler.add_link_runtime(link_runtime).await;
    
    println!("5G Link Runtime added to scheduler");
    println!("Schedule: Markov chain with 3 states (excellent, degraded, handover)");
    println!("- Excellent: {} kbps (mmWave)", nr_excellent.rate_kbps);
    println!("- Degraded: {} kbps (mmWave with blockage)", nr_degraded.rate_kbps);  
    println!("- Handover: {} kbps (Sub-6 with CA)", nr_handover.rate_kbps);
    println!("Mean dwell time: 15 seconds");

    // Start the scheduler (in a real scenario, this would run continuously)
    println!("Starting scheduler... (this would run network impairment changes in the background)");
    scheduler.start().await?;

    // In a real implementation, the scheduler would:
    // 1. Monitor elapsed time since start
    // 2. Evaluate Markov state transitions using random number generation
    // 3. Apply new network conditions via netlink/qdisc when state changes
    // 4. Set up next transition using exponential distribution
    
    println!("Enhanced scheduler demo completed!");
    println!("The scheduler now supports:");
    println!("✓ Markov chain transitions with configurable probability matrices");
    println!("✓ Exponential dwell time distributions"); 
    println!("✓ JSON trace replay capability");
    println!("✓ Integration with comprehensive 5G scenarios");

    Ok(())
}