//! Complete API demonstration for EnhancedNetworkOrchestrator
//!
//! This example shows:
//! - Race car cellular bonding setup  
//! - Real-time metrics collection
//! - Dynamic schedule application
//! - Full observability pipeline

use anyhow::Result;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::init();

    println!("=== Enhanced Network Orchestrator Complete API Demo ===\n");

    // Create enhanced orchestrator for race car testing
    let mut orchestrator = netlink_sim::enhanced::EnhancedNetworkOrchestrator::for_race_car_testing(
        "/tmp/race_car_session.trace"
    ).await?;

    println!("✓ Enhanced orchestrator created with observability\n");

    // Set up race car bonding scenario  
    let links = orchestrator.start_race_car_bonding(5004).await?;
    
    println!("✓ Race car bonding scenario started:");
    for (i, handle) in links.iter().enumerate() {
        println!("  Link {}: {} -> {} ({})", 
                i + 1, 
                handle.ingress_port, 
                handle.egress_port,
                handle.scenario.name);
    }
    println!();

    // Simulate some traffic and collect metrics
    println!("📊 Simulating traffic and collecting metrics...");
    for i in 0..5 {
        sleep(Duration::from_millis(500)).await;
        
        // Simulate updating metrics for each link
        for (idx, handle) in links.iter().enumerate() {
            let bytes_sent = 1000 * (i + 1) * (idx + 1) as u64;
            let bytes_recv = bytes_sent * 95 / 100; // 5% loss
            let rtt = 20.0 + (i as f64 * 5.0); // Increasing RTT
            let loss = 0.01 + (i as f64 * 0.01); // Increasing loss
            
            orchestrator.update_link_metrics(
                &handle.scenario.name,
                bytes_sent,
                bytes_recv, 
                rtt,
                loss
            ).await?;
        }
        
        print!(".");
    }
    println!(" Done!\n");

    // Apply dynamic schedule (race track with signal degradation)
    println!("🏁 Applying race track schedule pattern...");
    let race_schedule = scenarios::Schedule::race_track_circuit();
    
    orchestrator.apply_schedule("race_4g_primary", race_schedule).await?;
    println!("✓ Schedule applied to primary 4G link\n");

    // Take metrics snapshot
    if let Some(snapshot) = orchestrator.get_metrics_snapshot().await? {
        println!("📈 Current metrics snapshot:");
        println!("  Simulation ID: {}", snapshot.simulation_metrics.simulation_id);
        println!("  Timestamp: {:?}", snapshot.timestamp);
        println!("  Active Links: {}", snapshot.link_performance.len());
        
        for metrics in &snapshot.link_performance {
            println!("    {}: {}kbps throughput, {:.2}ms RTT", 
                    metrics.link_stats.link_id, 
                    metrics.link_stats.throughput_bps / 1000,
                    metrics.link_stats.rtt_ms);
        }
        println!();
    }

    // Demo alternative 5G research scenario
    println!("🚀 Starting 5G research scenario...");
    let mut research_orchestrator = netlink_sim::enhanced::EnhancedNetworkOrchestrator::for_5g_research(
        "/tmp/5g_research.trace"
    ).await?;
    
    let _5g_links = research_orchestrator.start_enhanced_5g_scenario(5005).await?;
    println!("✓ 5G research scenario with {} links active\n", _5g_links.len());

    // Show active links summary
    println!("📋 Final Status:");
    println!("  Race Car Orchestrator: {} active links", orchestrator.get_active_links().len());
    println!("  5G Research Orchestrator: {} active links", research_orchestrator.get_active_links().len());
    println!("\n✨ Complete API demonstration finished!");
    println!("📁 Trace files saved:");
    println!("  - /tmp/race_car_session.trace");  
    println!("  - /tmp/5g_research.trace");

    Ok(())
}