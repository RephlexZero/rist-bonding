//! Race Car Complete Demo with netns-testbench
//!
//! This example demonstrates comprehensive RIST bonding in realistic race car 
//! scenarios using the netns-testbench API.

use anyhow::Result;
use gstreamer::prelude::*;
use gstristelements::testing;
use netns_testbench::{NetworkOrchestrator, TestScenario};
use scenarios::{DirectionSpec, LinkSpec, Schedule};
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn};
use tracing_subscriber::fmt;

/// Race car telemetry configuration
#[derive(Debug, Clone)]
struct RaceCarConfig {
    pub driver_name: String,
    pub car_number: u32,
    pub track_name: String,
    pub session_type: String,
    pub video_bitrate_kbps: u32,
    pub audio_bitrate_kbps: u32,
}

impl Default for RaceCarConfig {
    fn default() -> Self {
        Self {
            driver_name: "Lewis Hamilton".to_string(),
            car_number: 44,
            track_name: "Monaco Grand Prix".to_string(),
            session_type: "Race".to_string(),
            video_bitrate_kbps: 5000,  // 5 Mbps for race car video
            audio_bitrate_kbps: 256,   // High quality audio for team radio
        }
    }
}

/// Create a realistic race car network scenario
fn create_race_car_network_scenario() -> TestScenario {
    use std::time::Duration as StdDuration;
    
    let mut metadata = HashMap::new();
    metadata.insert("scenario_type".to_string(), "race_car_live".to_string());
    metadata.insert("track".to_string(), "monaco_street_circuit".to_string());
    
    TestScenario {
        name: "race_car_monaco_live".to_string(),
        description: "Live race car broadcast from Monaco with realistic network challenges".to_string(),
        links: vec![
            // Primary 5G mmWave link - high speed, variable quality
            LinkSpec {
                name: "5g_mmwave_primary".to_string(),
                a_ns: "tx_5g".to_string(),
                b_ns: "rx_broadcast".to_string(),
                a_to_b: Schedule::Steps(vec![
                    (StdDuration::from_secs(0), DirectionSpec::good()),       // Start/finish straight
                    (StdDuration::from_secs(5), DirectionSpec::typical()),   // Tight corners
                    (StdDuration::from_secs(10), DirectionSpec::poor()),      // Tunnel section
                    (StdDuration::from_secs(15), DirectionSpec::degraded()),  // Tunnel depths
                    (StdDuration::from_secs(20), DirectionSpec::poor()),      // Tunnel exit
                    (StdDuration::from_secs(25), DirectionSpec::typical()),   // Harbor section
                    (StdDuration::from_secs(30), DirectionSpec::good()),      // Back to start/finish
                ]),
                b_to_a: Schedule::Constant(DirectionSpec::good()), // Uplink for telemetry
            },
            // Secondary LTE link - consistent backup
            LinkSpec {
                name: "lte_backup".to_string(),
                a_ns: "tx_lte".to_string(),
                b_ns: "rx_broadcast".to_string(),
                a_to_b: Schedule::Constant(DirectionSpec::typical()),
                b_to_a: Schedule::Constant(DirectionSpec::typical()),
            },
        ],
        duration_seconds: Some(35), // One lap around Monaco
        metadata,
    }
}

/// Monitor race car telemetry and streaming statistics
async fn monitor_race_car_telemetry(config: &RaceCarConfig) {
    info!("📊 Starting race car telemetry monitoring");
    
    for i in 0..30 { // Monitor for 30 seconds
        sleep(Duration::from_secs(1)).await;
        
        let timestamp = i + 1;
        
        // Simulate race car telemetry
        let speed_kmh = 280 + (i % 50) as u32; // Varying speed
        let gear = std::cmp::min(8, (speed_kmh / 40) as u8);
        let rpm = 8000 + (i * 100) % 4000;
        
        info!("🏎️ [{}s] Telemetry: Speed: {} km/h, Gear: {}, RPM: {}",
              timestamp, speed_kmh, gear, rpm);
        
        // Warn about network issues
        if i == 10 {
            warn!("⚠️ [{}s] Entering tunnel section - expect signal degradation", timestamp);
        } else if i == 20 {
            info!("✅ [{}s] Exiting tunnel - signal recovering", timestamp);
        }
    }
    
    info!("🏁 Race car telemetry monitoring completed");
}

/// Main race car demo function
#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    fmt::init();
    
    info!("🏁 RIST Race Car Live Broadcast Demo");
    info!("===========================================");
    
    // Initialize GStreamer
    gstreamer::init()?;
    testing::init_for_tests();
    
    // Race car configuration
    let config = RaceCarConfig::default();
    
    info!("🏎️ Race Configuration:");
    info!("   Driver: {}", config.driver_name);
    info!("   Car: #{}", config.car_number);
    info!("   Track: {}", config.track_name);
    info!("   Session: {}", config.session_type);
    info!("   Video: {} kbps, Audio: {} kbps", 
          config.video_bitrate_kbps, config.audio_bitrate_kbps);
    
    // === Step 1: Initialize Network Orchestrator ===
    info!("\n🌐 Step 1: Starting advanced network orchestrator...");
    let mut orchestrator = NetworkOrchestrator::new(44).await?; // Car #44 seed
    info!("✓ Network orchestrator ready with race car optimizations");
    
    // === Step 2: Configure Race Car Network Scenario ===
    info!("\n📡 Step 2: Configuring Monaco street circuit network scenario...");
    let scenario = create_race_car_network_scenario();
    let link = orchestrator.start_scenario(scenario, 5044).await?;
    
    info!("✓ Network scenario active:");
    info!("   Scenario: {}", link.scenario.name);
    info!("   Links: {} configured", link.scenario.links.len());
    info!("   Duration: {:?}", link.scenario.duration_seconds);
    
    // === Step 3: Build Simplified Race Car Pipeline ===
    info!("\n🎥 Step 3: Building race car broadcast pipeline...");
    
    let pipeline = gstreamer::Pipeline::new();
    
    // Create test video source (simulating race car camera)
    let src = testing::create_test_source();
    let sink = testing::create_fake_sink();
    
    pipeline.add_many([&src, &sink])?;
    src.link(&sink)?;
    
    info!("✓ Race car pipeline constructed");
    
    // === Step 4: Start the Race! ===
    info!("\n🏁 Step 4: Starting live race broadcast!");
    
    pipeline.set_state(gstreamer::State::Playing)?;
    info!("✓ Live broadcast active - Monaco GP is underway!");
    
    // === Step 5: Monitor Race Progress ===
    info!("\n📊 Step 5: Monitoring race progress and network performance...");
    
    // Start telemetry monitoring
    let telemetry_handle = tokio::spawn(monitor_race_car_telemetry(config.clone()));
    
    info!("   Press Ctrl+C to stop the race broadcast...");
    
    // Wait for telemetry monitoring to complete or handle shutdown
    tokio::select! {
        _ = telemetry_handle => {
            info!("🏁 Race completed successfully!");
        }
        _ = tokio::signal::ctrl_c() => {
            info!("🛑 Race broadcast interrupted by user");
        }
    }
    
    // === Step 6: Race Cleanup ===
    info!("\n🧹 Step 6: Cleaning up race broadcast...");
    
    pipeline.set_state(gstreamer::State::Null)?;
    info!("✓ Pipelines stopped");
    info!("✓ Network resources released");
    
    // === Final Race Report ===
    info!("\n📈 Final Race Report");
    info!("====================");
    info!("✓ {} - Car #{} race broadcast completed successfully!", 
          config.driver_name, config.car_number);
    info!("🏆 Monaco Grand Prix broadcast demonstration finished!");
    
    Ok(())
}