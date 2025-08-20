//! Integration tests for netns-testbench
//!
//! These tests verify that the scenarios work correctly and the crate can be imported.

use scenarios::DirectionSpec;
use tokio::time::Duration;

/// Initialize logging for tests
fn init_logging() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("netns_testbench=debug")
        .try_init();
}

#[tokio::test]
async fn test_scenario_compatibility() {
    init_logging();

    // Test that all preset scenarios are valid
    let scenarios = scenarios::Presets::all_scenarios();

    for scenario in scenarios {
        // Basic validation
        assert!(!scenario.name.is_empty());
        assert!(!scenario.description.is_empty());
        assert!(!scenario.links.is_empty());

        // Validate link configurations
        for link in &scenario.links {
            assert!(!link.name.is_empty());
            assert!(!link.a_ns.is_empty());
            assert!(!link.b_ns.is_empty());
        }

        println!("✅ Scenario '{}' is valid", scenario.name);
    }
}

#[tokio::test]
async fn test_5g_scenarios() {
    init_logging();

    // Test enhanced 5G scenarios
    let nr_scenarios = scenarios::Presets::nr_scenarios();

    assert!(!nr_scenarios.is_empty());

    for scenario in nr_scenarios {
        println!("Testing 5G scenario: {}", scenario.name);

        // Validate scenario structure
        assert!(!scenario.description.is_empty());
        assert!(!scenario.links.is_empty());

        // Validate links have proper configuration
        for link in &scenario.links {
            assert!(!link.name.is_empty());
            match &link.a_to_b {
                scenarios::Schedule::Constant(spec) => {
                    assert!(spec.rate_kbps > 0);
                }
                scenarios::Schedule::Steps(steps) => {
                    assert!(!steps.is_empty());
                }
                _ => {} // Other schedule types are valid too
            }
        }
    }

    println!("✅ All 5G scenarios are properly configured");
}

#[tokio::test]
async fn test_scenario_builder() {
    init_logging();

    // Test scenario builder functionality
    let scenario = scenarios::ScenarioBuilder::new("test")
        .description("Test scenario for integration testing")
        .duration(Duration::from_secs(30))
        .metadata("test_type", "integration")
        .metadata("environment", "ci")
        .build();

    assert_eq!(scenario.name, "test");
    assert!(!scenario.description.is_empty());
    assert_eq!(scenario.duration_seconds, Some(30));
    assert_eq!(
        scenario.metadata.get("test_type"),
        Some(&"integration".to_string())
    );
    assert_eq!(
        scenario.metadata.get("environment"),
        Some(&"ci".to_string())
    );

    println!("✅ Scenario builder works correctly");
}

#[tokio::test]
async fn test_direction_spec_effects() {
    init_logging();

    // Test 5G effect modifiers
    let base_spec = DirectionSpec::nr_sub6ghz();

    // Test carrier aggregation
    let ca_spec = base_spec.clone().with_carrier_aggregation(3);
    assert!(ca_spec.rate_kbps > base_spec.rate_kbps);

    // Test mmWave blockage
    let mmwave_spec = DirectionSpec::nr_mmwave();
    let blocked_spec = mmwave_spec.clone().with_mmwave_blockage(0.8);
    assert!(blocked_spec.loss_pct > mmwave_spec.loss_pct);
    assert!(blocked_spec.base_delay_ms > mmwave_spec.base_delay_ms);

    // Test beamforming effects
    let beam_spec = base_spec.clone().with_beamforming_steering(0.5);
    assert!(beam_spec.jitter_ms > base_spec.jitter_ms);

    // Test bufferbloat
    let buffer_spec = base_spec.clone().with_bufferbloat(0.3);
    assert!(buffer_spec.base_delay_ms > base_spec.base_delay_ms);

    println!("✅ Direction spec effect modifiers work correctly");
}

#[tokio::test]
async fn test_crate_imports() {
    init_logging();

    // Test that we can import and use the crate types
    use netns_testbench::{NetworkOrchestrator, TestbenchError};

    // Test error types
    let error = TestbenchError::InvalidConfig("test".to_string());
    assert!(error.to_string().contains("test"));

    // Test orchestrator creation
    let result = NetworkOrchestrator::new(12345).await;
    match result {
        Ok(orchestrator) => {
            println!("✅ NetworkOrchestrator created successfully");
            // Note: We can't test apply_scenario without proper privileges
            drop(orchestrator);
        }
        Err(e) => {
            println!(
                "⚠️  NetworkOrchestrator creation failed (likely permissions): {}",
                e
            );
        }
    }

    println!("✅ Crate imports work correctly");
}
