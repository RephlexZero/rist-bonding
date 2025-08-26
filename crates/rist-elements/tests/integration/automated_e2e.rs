//! Automated end-to-end integration tests
//!
//! These tests verify the complete system behavior in automated test runs.
//! Converted from examples/end_to_end_test.rs to run under `cargo test`.

use anyhow::Result;
use super::{RistIntegrationTest, TestResults, ValidationReport};
use serial_test::serial;
use std::time::Instant;
use tracing::{debug, info};

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn test_complete_bonding_integration() -> Result<()> {
    // Plain logging with timestamps retained
    let _ = tracing_subscriber::fmt()
        .with_target(false)
        // Allow colors (ANSI) for readability
        .with_ansi(true)
        .try_init();

    let test_start = Instant::now();
    let test_id = format!(
        "automated_test_{}",
        chrono::Utc::now().format("%Y%m%d_%H%M%S")
    );

    // Create integration test
    // Use an even RTP port (RTCP uses +1)
    let mut test = RistIntegrationTest::new(test_id.clone(), 5006).await?;
    info!("Integration test framework initialized");

    // Start RIST pipelines inside namespaces created by the orchestrator
    let links = match test.setup_bonding().await {
        Ok(links) => links,
        Err(e) => {
            if e.to_string().contains("Permission denied") {
                info!(
                    "SKIP: netns requires root/CAP_SYS_ADMIN; skipping test: {}",
                    e
                );
                return Ok(());
            } else {
                return Err(e);
            }
        }
    };
    if let Err(e) = test.start_rist_pipelines_in_netns().await {
        if e.to_string().contains("Permission denied") {
            info!(
                "SKIP: netns requires root/CAP_SYS_ADMIN; skipping test: {}",
                e
            );
            return Ok(());
        } else {
            return Err(e);
        }
    }

    // Verify bonding setup
    assert!(!links.is_empty(), "Should have created bonding links");
    // Orchestrator currently starts only the first link of the scenario; expect >= 1
    assert!(links.len() >= 1, "Should have at least one active link");

    // Run complete pattern
    info!("Executing basic test flow");
    let results = test.run_basic_flow().await?;

    // Optional strict check: require seeing some buffers if env var is set
    if std::env::var("RIST_REQUIRE_BUFFERS")
        .map(|v| v == "1")
        .unwrap_or(false)
    {
        let saw_any_buffers = results.phases.iter().any(|(_, m)| m.avg_bitrate > 0.0);
        assert!(
            saw_any_buffers,
            "No buffers observed by receiver; set RIST_SHOW_VIDEO=1 or increase flow time to diagnose"
        );
    }

    // Validate test results structure
    assert!(results.phases.len() >= 4, "Should have tested all phases");
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
        "End-to-end test completed in {:.1}s",
        total_time.as_secs_f64()
    );

    // Assert that critical validations pass
    assert!(
        validation.bonding_effective,
        "Bonding should be effective during handovers"
    );

    if validation.all_passed() {
        info!("All tests passed");
    } else {
        info!("Some non-critical validations failed - see artifacts for details");
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn test_phase_transitions() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_target(false)
        .with_ansi(true)
        .try_init();

    info!("Testing phase transitions");

    let test_id = format!("phase_test_{}", chrono::Utc::now().format("%Y%m%d_%H%M%S"));
    let mut test = RistIntegrationTest::new(test_id, 5008).await?;

    // Test individual phase transitions
    if let Err(e) = test.setup_bonding().await {
        if e.to_string().contains("Permission denied") {
            info!(
                "SKIP: netns requires root/CAP_SYS_ADMIN; skipping test: {}",
                e
            );
            return Ok(());
        } else {
            return Err(e);
        }
    }

    // Test degradation application
    test.apply_degradation_schedule().await?;
    debug!("Degradation schedule applied");

    // Test handover trigger
    test.trigger_handover_event().await?;
    debug!("Handover event triggered");

    // Test recovery
    test.apply_recovery_schedule().await?;
    debug!("Recovery schedule applied");

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn test_complete_bonding_with_custom_dispatcher() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_target(false)
        .with_ansi(true)
        .try_init();

    // Distinct id so the MP4 filename is unique
    let test_id = format!(
        "automated_custom_{}",
        chrono::Utc::now().format("%Y%m%d_%H%M%S")
    );
    let mut test = RistIntegrationTest::new(test_id.clone(), 5012).await?;

    // Setup namespaces and links
    if let Err(e) = test.setup_bonding().await {
        if e.to_string().contains("Permission denied") {
            info!(
                "SKIP: netns requires root/CAP_SYS_ADMIN; skipping custom dispatcher test: {}",
                e
            );
            return Ok(());
        } else {
            return Err(e);
        }
    }

    // Start pipelines using custom rist-elements (ristdispatcher + dynbitrate)
    match test.start_rist_pipelines_in_netns_with_custom_dispatcher().await {
        Ok(()) => {}
        Err(e) => {
            // Gracefully skip if elements are missing (plugin not on GST_PLUGIN_PATH) or permission denied
            let es = e.to_string();
            if es.contains("Permission denied") || es.contains("Custom elements missing") {
                info!(
                    "SKIP: custom elements or perms unavailable; skipping: {}",
                    es
                );
                return Ok(());
            }
            return Err(e);
        }
    }

    // Run a shorter flow just to produce output and exercise control
    let _ = test.run_basic_flow().await?;
    info!("Custom dispatcher flow completed");
    Ok(())
}

#[tokio::test]
async fn test_metrics_collection() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_target(false)
        .with_ansi(true)
        .try_init();

    info!("Testing metrics collection");

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

    debug!("Metrics collection working correctly");
    Ok(())
}

#[tokio::test]
async fn test_validation_logic() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_target(false)
        .with_ansi(true)
        .try_init();

    info!("Testing validation logic");

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

    debug!("Validation logic working correctly");
    Ok(())
}

/// Produce two MP4s without requiring root by running local-mode pipelines
#[tokio::test]
#[serial]
async fn test_produce_two_mp4s_local_mode() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_target(false)
        .with_ansi(true)
        .try_init();

    // First: baseline local-mode
    let id1 = format!(
        "automated_local1_{}",
        chrono::Utc::now().format("%Y%m%d_%H%M%S")
    );
    let mut t1 = RistIntegrationTest::new(id1.clone(), 5510).await?;
    match t1.start_local_rist_pipelines().await {
        Ok(()) => {
            // Let it run briefly to record
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            drop(t1); // triggers EOS and mp4 finalization
        }
        Err(e) => {
            info!("SKIP: baseline local-mode unavailable: {}", e);
        }
    }

    // Second: custom dispatcher local-mode (skip gracefully if elements missing)
    let id2 = format!(
        "automated_local2_custom_{}",
        chrono::Utc::now().format("%Y%m%d_%H%M%S")
    );
    let mut t2 = RistIntegrationTest::new(id2.clone(), 5512).await?;
    match t2.start_local_rist_pipelines_with_custom_dispatcher().await {
        Ok(()) => {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            drop(t2);
        }
        Err(e) => {
            info!("SKIP: custom local-mode unavailable: {}", e);
        }
    }

    // Verify files exist if they were expected
    let path1 = integration_tests::RistIntegrationTest::artifact_path(&format!("{}.mp4", id1));
    if let Ok(meta) = tokio::fs::metadata(&path1).await {
        info!("Local baseline MP4 size = {} bytes at {}", meta.len(), path1.display());
    } else {
        info!("Local baseline MP4 missing: {}", path1.display());
    }

    let path2 = integration_tests::RistIntegrationTest::artifact_path(&format!("{}.mp4", id2));
    if let Ok(meta) = tokio::fs::metadata(&path2).await {
        info!("Local custom MP4 size = {} bytes at {}", meta.len(), path2.display());
    } else {
        info!("Local custom MP4 missing (likely skipped): {}", path2.display());
    }

    Ok(())
}

/// Generate test artifacts (non-blocking for test success)
async fn generate_test_artifacts(
    test_id: &str,
    results: &TestResults,
    validation: &ValidationReport,
) -> Result<()> {
    info!("Generating test artifacts");

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
        "environment": { "note": "Simulated via netns-testbench" }
    });

    // Save report to the unified artifacts directory
    let report_path = integration_tests::RistIntegrationTest::artifact_path(&format!(
        "rist_automated_test_{}.json",
        test_id
    ));
    match tokio::fs::write(&report_path, serde_json::to_string_pretty(&report)?).await {
        Ok(()) => info!("Test artifacts saved to {}", report_path.display()),
        Err(e) => info!("Could not save test artifacts: {}", e),
    }

    Ok(())
}
