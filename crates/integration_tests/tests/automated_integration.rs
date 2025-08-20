//! Automated end-to-end integration tests
//!
//! These tests verify the complete system behavior in automated test runs.
//! Converted from examples/end_to_end_test.rs to run under `cargo test`.

use anyhow::Result;
use integration_tests::{RistIntegrationTest, TestResults, ValidationReport};
use std::time::Instant;
use tracing::{debug, info};
use tracing_subscriber::fmt;

#[tokio::test]
async fn test_complete_race_car_integration() -> Result<()> {
    // Initialize logging for this test
    let _ = fmt::try_init();

    info!("🏁 RIST Bonding End-to-End Integration Test");
    info!("============================================");

    let test_start = Instant::now();
    let test_id = format!(
        "automated_test_{}",
        chrono::Utc::now().format("%Y%m%d_%H%M%S")
    );

    // Create integration test
    let mut test = RistIntegrationTest::new(test_id.clone(), 5007).await?;
    info!("✓ Integration test framework initialized");

    // Start RIST dispatcher
    test.start_rist_dispatcher().await?;

    // Set up race car bonding
    let links = test.setup_race_car_bonding().await?;

    // Verify bonding setup
    assert!(!links.is_empty(), "Should have created bonding links");
    assert!(
        links.len() >= 2,
        "Should have at least 2 bonding links for redundancy"
    );

    // Run complete race car test pattern
    info!("🚗 Executing race car test scenario...");
    let results = test.run_race_car_test_pattern().await?;

    // Validate test results structure
    assert!(
        results.phases.len() >= 4,
        "Should have tested all race car phases"
    );
    assert!(
        results.total_duration.as_secs() > 0,
        "Test should have measurable duration"
    );

    // Validate bonding behavior
    let validation = test.validate_bonding_behavior(&results).await?;

    // Generate test artifacts (but don't require success for test pass)
    let _ = generate_test_artifacts(&test_id, &results, &validation).await;

    let total_time = test_start.elapsed();
    info!(
        "🏆 End-to-end test completed in {:.1}s",
        total_time.as_secs_f64()
    );

    // Assert that critical validations pass
    assert!(
        validation.bonding_effective,
        "Bonding should be effective during handovers"
    );

    if validation.all_passed() {
        info!("✅ ALL TESTS PASSED - RIST bonding system is working correctly!");
    } else {
        info!("⚠️ Some non-critical validations failed - see artifacts for details");
    }

    Ok(())
}

#[tokio::test]
async fn test_race_car_phase_transitions() -> Result<()> {
    let _ = fmt::try_init();

    info!("🔄 Testing race car phase transitions");

    let test_id = format!("phase_test_{}", chrono::Utc::now().format("%Y%m%d_%H%M%S"));
    let mut test = RistIntegrationTest::new(test_id, 5008).await?;

    // Test individual phase transitions
    test.setup_race_car_bonding().await?;

    // Test degradation application
    test.apply_degradation_schedule().await?;
    debug!("✓ Degradation schedule applied");

    // Test handover trigger
    test.trigger_handover_event().await?;
    debug!("✓ Handover event triggered");

    // Test recovery
    test.apply_recovery_schedule().await?;
    debug!("✓ Recovery schedule applied");

    Ok(())
}

#[tokio::test]
async fn test_metrics_collection() -> Result<()> {
    let _ = fmt::try_init();

    info!("📊 Testing metrics collection");

    let test_id = format!(
        "metrics_test_{}",
        chrono::Utc::now().format("%Y%m%d_%H%M%S")
    );
    let test = RistIntegrationTest::new(test_id, 5009).await?;

    // Test metrics collection
    let metrics = test.collect_phase_metrics().await?;

    // Validate metrics structure
    assert!(
        metrics.avg_bitrate >= 0.0,
        "Average bitrate should be non-negative"
    );
    assert!(
        metrics.packet_loss >= 0.0 && metrics.packet_loss <= 100.0,
        "Packet loss should be a valid percentage"
    );
    assert!(metrics.avg_rtt >= 0.0, "Average RTT should be non-negative");

    debug!("✓ Metrics collection working correctly");
    Ok(())
}

#[tokio::test]
async fn test_validation_logic() -> Result<()> {
    let _ = fmt::try_init();

    info!("🔍 Testing validation logic");

    // Create test results with known characteristics
    let mut results = TestResults::new("validation_test".to_string());

    // Add phases with different characteristics to test validation
    results.add_phase(
        "strong",
        integration_tests::PhaseMetrics {
            avg_bitrate: 2000.0,
            packet_loss: 0.1,
            avg_rtt: 20.0,
            primary_link_util: 80.0,
            backup_link_util: 20.0,
        },
    );

    results.add_phase(
        "degraded",
        integration_tests::PhaseMetrics {
            avg_bitrate: 800.0, // Below 1000 threshold
            packet_loss: 2.0,
            avg_rtt: 150.0,
            primary_link_util: 60.0,
            backup_link_util: 40.0,
        },
    );

    results.add_phase(
        "handover",
        integration_tests::PhaseMetrics {
            avg_bitrate: 1200.0,
            packet_loss: 3.0, // Below 5% threshold
            avg_rtt: 100.0,
            primary_link_util: 50.0,
            backup_link_util: 50.0,
        },
    );

    results.add_phase(
        "recovery",
        integration_tests::PhaseMetrics {
            avg_bitrate: 1800.0, // Above 1500 threshold
            packet_loss: 0.5,
            avg_rtt: 25.0,
            primary_link_util: 75.0,
            backup_link_util: 25.0,
        },
    );

    let test_id = format!(
        "validation_test_{}",
        chrono::Utc::now().format("%Y%m%d_%H%M%S")
    );
    let test = RistIntegrationTest::new(test_id, 5010).await?;

    let validation = test.validate_bonding_behavior(&results).await?;

    // Test validation logic
    assert!(
        validation.adaptive_bitrate_working,
        "Should detect adaptive bitrate from test data"
    );
    assert!(
        validation.bonding_effective,
        "Should detect effective bonding from test data"
    );
    assert!(
        validation.all_passed(),
        "All validations should pass with good test data"
    );

    debug!("✓ Validation logic working correctly");
    Ok(())
}

/// Generate test artifacts (non-blocking for test success)
async fn generate_test_artifacts(
    test_id: &str,
    results: &TestResults,
    validation: &ValidationReport,
) -> Result<()> {
    info!("📊 Generating test artifacts...");

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

    // Try to save report to temp directory
    let report_path = format!("/tmp/rist_automated_test_{}.json", test_id);
    match tokio::fs::write(&report_path, serde_json::to_string_pretty(&report)?).await {
        Ok(()) => info!("✓ Test artifacts saved to {}", report_path),
        Err(e) => info!("⚠️ Could not save test artifacts: {}", e),
    }

    Ok(())
}
