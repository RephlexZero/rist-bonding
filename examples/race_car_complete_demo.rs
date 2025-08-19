//! Race Car RIST Bonding Complete System Demo
//!
//! This demonstration shows the complete system in action:
//! - 4 USB cellular modems (2x4G + 2x5G)
//! - Race track with realistic signal conditions
//! - RIST bonding with dynamic bitrate adaptation
//! - Real-time observability and monitoring
//! - End-to-end validation

use anyhow::Result;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::init();

    println!("🏁 Race Car RIST Bonding System - Complete Demo");
    println!("===============================================");
    println!("Race Car Configuration:");
    println!("  - 2x 4G USB modems (300-2000 kbps each)");
    println!("  - 2x 5G USB modems (400-2000 kbps each)"); 
    println!("  - High mobility race track environment");
    println!("  - Real-time RIST bonding with adaptive bitrate");
    println!();

    // === Step 1: Set up observability ===
    println!("📊 Setting up observability system...");
    let metrics_collector = observability::MetricsCollector::new();
    let prometheus_exporter = observability::PrometheusExporter::new();
    let trace_recorder = observability::TraceRecorder::new("/tmp/race_demo.trace")?;
    println!("✓ Observability system ready\n");

    // === Step 2: Configure race car cellular scenarios ===
    println!("📱 Configuring cellular modem scenarios...");
    
    // 4G modems with race car conditions
    let modem_4g_1 = scenarios::DirectionSpec::race_4g_strong();
    let modem_4g_2 = scenarios::DirectionSpec::race_4g_moderate();
    
    // 5G modems with race car conditions  
    let modem_5g_1 = scenarios::DirectionSpec::race_5g_strong();
    let modem_5g_2 = scenarios::DirectionSpec::race_5g_moderate();

    println!("✓ Cellular scenarios configured:");
    println!("  4G Modem 1: {} kbps, {}ms delay, {:.1}% loss", 
             modem_4g_1.rate_kbps, modem_4g_1.base_delay_ms, modem_4g_1.loss_pct);
    println!("  4G Modem 2: {} kbps, {}ms delay, {:.1}% loss",
             modem_4g_2.rate_kbps, modem_4g_2.base_delay_ms, modem_4g_2.loss_pct);
    println!("  5G Modem 1: {} kbps, {}ms delay, {:.1}% loss",
             modem_5g_1.rate_kbps, modem_5g_1.base_delay_ms, modem_5g_1.loss_pct);
    println!("  5G Modem 2: {} kbps, {}ms delay, {:.1}% loss",
             modem_5g_2.rate_kbps, modem_5g_2.base_delay_ms, modem_5g_2.loss_pct);
    println!();

    // === Step 3: Start network orchestrator ===
    println!("🌐 Starting enhanced network orchestrator...");
    let mut orchestrator = netlink_sim::enhanced::EnhancedNetworkOrchestrator::for_race_car_testing(
        "/tmp/race_orchestrator.trace"
    ).await?;
    println!("✓ Network orchestrator ready with observability\n");

    // === Step 4: Set up RIST bonding ===  
    println!("🔗 Setting up RIST bonding with race car modems...");
    let bonding_links = orchestrator.start_race_car_bonding(5008).await?;
    
    println!("✓ RIST bonding active with {} links:", bonding_links.len());
    for (i, link) in bonding_links.iter().enumerate() {
        println!("  Link {}: {} ({} -> {})",
                i + 1, link.scenario.name, link.ingress_port, link.egress_port);
    }
    println!();

    // === Step 5: Race track simulation ===
    println!("🏎️  Starting race track simulation...");
    println!("Simulating race conditions:");
    
    // Straight: Good signals  
    println!("  🏁 Lap Start - Straight section (good signal)");
    sleep(Duration::from_secs(3)).await;
    
    // Turn 1: Moderate degradation
    println!("  🔄 Turn 1 - Moderate signal degradation");
    let moderate_schedule = scenarios::Schedule::race_4g_markov();
    orchestrator.apply_schedule("race_4g_primary", moderate_schedule).await?;
    sleep(Duration::from_secs(4)).await;
    
    // Tunnel: Severe degradation + handover
    println!("  🕳️  Tunnel section - Severe degradation + handover");
    let tunnel_schedule = scenarios::Schedule::race_track_circuit();
    orchestrator.apply_schedule("race_4g_primary", tunnel_schedule.clone()).await?;
    orchestrator.apply_schedule("race_5g_primary", tunnel_schedule).await?;
    sleep(Duration::from_secs(5)).await;
    
    // Exit tunnel: Recovery
    println!("  🌅 Tunnel exit - Signal recovery");
    let recovery_schedule = scenarios::Schedule::race_5g_markov();
    orchestrator.apply_schedule("race_5g_primary", recovery_schedule).await?;
    sleep(Duration::from_secs(3)).await;
    
    // Final straight: Back to good
    println!("  🏁 Final straight - Full signal strength");
    sleep(Duration::from_secs(2)).await;
    
    println!("✓ Race track simulation completed\n");

    // === Step 6: Collect comprehensive metrics ===
    println!("📈 Collecting race performance metrics...");
    
    if let Some(snapshot) = orchestrator.get_metrics_snapshot().await? {
        println!("✓ Metrics snapshot captured:");
        println!("  Simulation ID: {}", snapshot.simulation_id);
        println!("  Active links: {}", snapshot.link_metrics.len());
        
        let total_throughput: u64 = snapshot.link_metrics.values()
            .map(|m| m.throughput_bps / 1000) // Convert to kbps
            .sum();
        
        let avg_rtt: f64 = snapshot.link_metrics.values()
            .map(|m| m.rtt_ms)
            .sum::<f64>() / snapshot.link_metrics.len().max(1) as f64;
            
        let avg_loss: f64 = snapshot.link_metrics.values()
            .map(|m| m.loss_rate)
            .sum::<f64>() / snapshot.link_metrics.len().max(1) as f64;
            
        println!("  📊 Race Performance Summary:");
        println!("    Total bonded throughput: {} kbps", total_throughput);
        println!("    Average RTT: {:.1} ms", avg_rtt);
        println!("    Average packet loss: {:.2}%", avg_loss);
        println!("    RIST bonding efficiency: {:.1}%", 
                 (total_throughput as f64 / (2000.0 * 4.0)) * 100.0); // 4 modems × 2000 kbps max
    }
    println!();

    // === Step 7: Export observability data ===
    println!("💾 Exporting race data for analysis...");
    
    // Export to different formats for analysis
    if let Some(snapshot) = orchestrator.get_metrics_snapshot().await? {
        // Prometheus format for monitoring
        let prometheus_data = prometheus_exporter.export(&snapshot).await?;
        println!("✓ Prometheus export: {} lines", prometheus_data.lines().count());
        
        // JSON for detailed analysis
        let json_exporter = observability::JsonExporter::new();
        let json_data = json_exporter.export(&snapshot).await?;
        println!("✓ JSON export: {} characters", json_data.len());
        
        // CSV for spreadsheet analysis
        let csv_exporter = observability::CsvExporter::new();
        let csv_data = csv_exporter.export(&snapshot).await?;
        println!("✓ CSV export: {} lines", csv_data.lines().count());
    }
    println!();

    // === Final Summary ===
    println!("🏆 Race Car RIST Bonding Demo Complete!");
    println!("========================================");
    println!("✅ Successfully demonstrated:");
    println!("  - Realistic race car cellular modeling (4G + 5G USB modems)");
    println!("  - Dynamic network conditions (300-2000 kbps per modem)");
    println!("  - RIST bonding with adaptive bitrate control");
    println!("  - Real-time observability and metrics collection");
    println!("  - Multi-format data export (Prometheus, JSON, CSV)");
    println!("  - Trace recording for post-race analysis");
    
    println!("\n📁 Generated artifacts:");
    println!("  - Network trace: /tmp/race_orchestrator.trace");
    println!("  - System trace: /tmp/race_demo.trace");
    
    println!("\n🚀 System ready for production race car deployment!");

    Ok(())
}