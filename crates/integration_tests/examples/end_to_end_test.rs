//! Complete end-to-end integration test runner
//!
//! This test runs the full system:
//! - Race car cellular modeling
//! - RIST dispatcher with bonding
//! - Real-time observability 
//! - Comprehensive validation

use anyhow::Result;
use tracing_subscriber::fmt;
use integration_tests::{RistIntegrationTest, TestResults, ValidationReport};
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    fmt::init();

    println!("🏁 RIST Bonding End-to-End Integration Test");
    println!("============================================\n");

    let test_start = Instant::now();
    let test_id = format!("race_car_{}", chrono::Utc::now().format("%Y%m%d_%H%M%S"));
    
    // Create integration test
    let mut test = RistIntegrationTest::new(test_id.clone(), 5007).await?;
    println!("✓ Integration test framework initialized\n");

    // Start RIST dispatcher
    test.start_rist_dispatcher().await?;

    // Set up race car bonding
    let _links = test.setup_race_car_bonding().await?;

    // Run complete race car test pattern
    println!("🚗 Executing race car test scenario...");
    let results = test.run_race_car_test_pattern().await?;

    // Validate bonding behavior
    let validation = test.validate_bonding_behavior(&results).await?;

    // Generate comprehensive report
    generate_test_report(&test_id, &results, &validation).await?;

    let total_time = test_start.elapsed();
    println!("🏆 End-to-end test completed in {:.1}s", total_time.as_secs_f64());
    
    if validation.all_passed() {
        println!("✅ ALL TESTS PASSED - RIST bonding system is working correctly!");
    } else {
        println!("❌ Some tests failed - see report for details");
    }

    println!("\n📁 Test artifacts saved:");
    println!("  - Trace file: /tmp/rist_test_{}.trace", test_id);
    println!("  - Report: /tmp/rist_integration_report_{}.json", test_id);

    Ok(())
}

async fn generate_test_report(
    test_id: &str,
    results: &TestResults,
    validation: &ValidationReport
) -> Result<()> {
    println!("📊 Generating comprehensive test report...");

    let report = serde_json::json!({
        "test_id": test_id,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "duration_seconds": results.total_duration.as_secs_f64(),
        "phases": results.phases.iter().map(|(name, metrics)| {
            serde_json::json!({
                "name": name,
                "avg_bitrate_kbps": metrics.avg_bitrate,
                "packet_loss_percent": metrics.packet_loss,
                "avg_rtt_ms": metrics.avg_rtt,
                "primary_utilization": metrics.primary_link_util,
                "backup_utilization": metrics.backup_link_util
            })
        }).collect::<Vec<_>>(),
        "validation": {
            "adaptive_bitrate_working": validation.adaptive_bitrate_working,
            "bonding_effective": validation.bonding_effective,
            "load_balancing_working": validation.load_balancing_working,
            "all_passed": validation.all_passed()
        },
        "race_car_conditions": {
            "cellular_modems": "2x4G + 2x5G USB modems",
            "bitrate_range": "300-2000 kbps per link",
            "mobility": "High speed race car",
            "environment": "Race track with tunnels and elevation changes"
        }
    });

    // Save report to file
    let report_path = format!("/tmp/rist_integration_report_{}.json", test_id);
    tokio::fs::write(&report_path, serde_json::to_string_pretty(&report)?).await?;

    // Print summary
    println!("✓ Integration test report generated");
    println!("\n📈 Test Summary:");
    println!("  Test ID: {}", test_id);
    println!("  Duration: {:.1}s", results.total_duration.as_secs_f64());
    println!("  Phases tested: {}", results.phases.len());
    
    for (phase, metrics) in &results.phases {
        println!("    {}: {:.0} kbps avg, {:.1}% loss, {:.1}ms RTT", 
                phase, metrics.avg_bitrate, metrics.packet_loss, metrics.avg_rtt);
    }

    println!("\n🔍 Validation Results:");
    println!("  Adaptive bitrate: {}", if validation.adaptive_bitrate_working { "✅ PASS" } else { "❌ FAIL" });
    println!("  Bonding effectiveness: {}", if validation.bonding_effective { "✅ PASS" } else { "❌ FAIL" });  
    println!("  Load balancing: {}", if validation.load_balancing_working { "✅ PASS" } else { "❌ FAIL" });

    Ok(())
}