//! CLI command implementations for the network testbench
//!
//! This module contains the implementations of all CLI commands,
//! extracted from main.rs to enable unit testing and better organization.

use anyhow::Result;
use netns_testbench::{NetworkOrchestrator, TestScenario};
use scenarios::Presets;
use std::time::Duration;
use tokio::signal;
use tokio::time::sleep;
use tracing::{error, info};

/// Implementation of the 'up' command - brings up network links with preset or scenario
pub async fn cmd_up(links: u8, preset: Option<String>, duration: u64, rx_port: u16) -> Result<()> {
    info!("Bringing up {} links for {} seconds", links, duration);

    let mut orchestrator = NetworkOrchestrator::new(42).await?;

    // Determine which scenario to use
    let scenario = resolve_preset(preset)?;

    // Start the scenario
    let handle = orchestrator.start_scenario(scenario, rx_port).await?;

    info!("Started scenario: {}", handle.scenario.name);
    info!("  Ingress Port: {}", handle.ingress_port);
    info!("  Egress Port:  {}", handle.egress_port);
    info!("  RX Port:      {}", handle.rx_port);

    // Start the runtime scheduler
    orchestrator.start_scheduler().await?;

    // Run for the specified duration or until interrupted
    tokio::select! {
        _ = sleep(Duration::from_secs(duration)) => {
            info!("Duration completed");
        }
        _ = signal::ctrl_c() => {
            info!("Interrupted by user");
        }
    }

    orchestrator.shutdown().await?;
    info!("Testbench shut down successfully");

    Ok(())
}

/// Implementation of the 'run' command - runs a specific scenario from file or preset
pub async fn cmd_run(scenario: String, rx_port: u16) -> Result<()> {
    info!("Running scenario: {}", scenario);

    let test_scenario = resolve_scenario(&scenario)?;

    let mut orchestrator = NetworkOrchestrator::new(42).await?;
    let handle = orchestrator.start_scenario(test_scenario, rx_port).await?;

    info!("Running scenario: {}", handle.scenario.name);
    info!("Description: {}", handle.scenario.description);

    // Start scheduler
    orchestrator.start_scheduler().await?;

    // Run for scenario duration or until interrupted
    let duration = handle.scenario.duration_seconds.unwrap_or(60);

    tokio::select! {
        _ = sleep(Duration::from_secs(duration)) => {
            info!("Scenario completed");
        }
        _ = signal::ctrl_c() => {
            info!("Interrupted by user");
        }
    }

    orchestrator.shutdown().await?;
    Ok(())
}

/// Implementation of the 'list' command - shows available scenarios and presets
pub async fn cmd_list() -> Result<()> {
    println!("Available scenarios:");
    println!("==================");

    println!("\nBasic scenarios:");
    for scenario in Presets::basic_scenarios() {
        println!("  {:<20} - {}", scenario.name, scenario.description);
    }

    println!("\nCellular scenarios:");
    for scenario in Presets::cellular_scenarios() {
        println!("  {:<20} - {}", scenario.name, scenario.description);
    }

    println!("\nMulti-link scenarios:");
    for scenario in Presets::multi_link_scenarios() {
        println!("  {:<20} - {}", scenario.name, scenario.description);
    }

    println!("\nBuilt-in presets:");
    println!("  good              - High quality baseline");
    println!("  poor              - Degraded network conditions");
    println!("  lte               - Mobile/cellular characteristics");
    println!("  bonding           - Dual-link bonding test");

    Ok(())
}

/// Implementation of the 'stats' command - shows live statistics
pub async fn cmd_stats(_interval: u64) -> Result<()> {
    // TODO: Implement live statistics display
    println!("Live statistics display not yet implemented");
    println!("This would show:");
    println!("  - Active links and their current parameters");
    println!("  - Packet counts and rates");
    println!("  - Current impairment settings");
    println!("  - Schedule progress");

    Ok(())
}

/// Helper function to resolve preset names to test scenarios
fn resolve_preset(preset: Option<String>) -> Result<TestScenario> {
    match preset.as_deref() {
        Some("good") => Ok(TestScenario::baseline_good()),
        Some("poor") => Ok(TestScenario::degrading_network()),
        Some("lte") => Ok(TestScenario::mobile_handover()),
        Some("bonding") => Ok(TestScenario::bonding_asymmetric()),
        Some(name) => {
            error!("Unknown preset: {}", name);
            anyhow::bail!("Unknown preset: {}", name);
        }
        None => Ok(TestScenario::baseline_good()),
    }
}

/// Helper function to resolve scenario names to test scenarios
fn resolve_scenario(scenario: &str) -> Result<TestScenario> {
    match scenario {
        "baseline_good" => Ok(TestScenario::baseline_good()),
        "bonding_asymmetric" => Ok(TestScenario::bonding_asymmetric()),
        "mobile_handover" => Ok(TestScenario::mobile_handover()),
        "degrading_network" => Ok(TestScenario::degrading_network()),
        "nr_to_lte_handover" => Ok(TestScenario::nr_to_lte_handover()),
        "nr_mmwave_mobility" => Ok(TestScenario::nr_mmwave_mobility()),
        "nr_network_slicing" => Ok(TestScenario::nr_network_slicing()),
        "nr_carrier_aggregation_test" => Ok(TestScenario::nr_carrier_aggregation_test()),
        "nr_beamforming_interference" => Ok(TestScenario::nr_beamforming_interference()),
        _ => {
            // Try to load from file
            error!("Scenario file loading not yet implemented");
            anyhow::bail!(
                "Scenario '{}' not found. File loading not yet implemented.",
                scenario
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_preset() {
        // Test valid presets
        assert!(resolve_preset(Some("good".to_string())).is_ok());
        assert!(resolve_preset(Some("poor".to_string())).is_ok());
        assert!(resolve_preset(Some("lte".to_string())).is_ok());
        assert!(resolve_preset(Some("bonding".to_string())).is_ok());
        assert!(resolve_preset(None).is_ok());

        // Test invalid preset
        assert!(resolve_preset(Some("invalid".to_string())).is_err());

        // Test specific preset resolution
        let good_scenario = resolve_preset(Some("good".to_string())).unwrap();
        assert_eq!(good_scenario.name, "baseline_good");

        let default_scenario = resolve_preset(None).unwrap();
        assert_eq!(default_scenario.name, "baseline_good");
    }

    #[test]
    fn test_resolve_scenario() {
        // Test valid scenarios
        assert!(resolve_scenario("baseline_good").is_ok());
        assert!(resolve_scenario("bonding_asymmetric").is_ok());
        assert!(resolve_scenario("mobile_handover").is_ok());
        assert!(resolve_scenario("degrading_network").is_ok());
        assert!(resolve_scenario("nr_to_lte_handover").is_ok());
        assert!(resolve_scenario("nr_mmwave_mobility").is_ok());
        assert!(resolve_scenario("nr_network_slicing").is_ok());
        assert!(resolve_scenario("nr_carrier_aggregation_test").is_ok());
        assert!(resolve_scenario("nr_beamforming_interference").is_ok());

        // Test invalid scenario
        assert!(resolve_scenario("nonexistent_scenario").is_err());

        // Test specific scenario resolution
        let scenario = resolve_scenario("baseline_good").unwrap();
        assert_eq!(scenario.name, "baseline_good");

        let mobile_scenario = resolve_scenario("mobile_handover").unwrap();
        assert_eq!(mobile_scenario.name, "mobile_handover");
    }

    #[test]
    fn test_all_preset_names_are_valid() {
        // Test that all built-in preset names resolve correctly
        let preset_names = vec!["good", "poor", "lte", "bonding"];

        for preset_name in preset_names {
            let result = resolve_preset(Some(preset_name.to_string()));
            assert!(result.is_ok(), "Preset '{}' should be valid", preset_name);

            let scenario = result.unwrap();
            assert!(
                !scenario.name.is_empty(),
                "Preset '{}' should have a name",
                preset_name
            );
            assert!(
                !scenario.links.is_empty(),
                "Preset '{}' should have links",
                preset_name
            );
        }
    }

    #[test]
    fn test_all_scenario_names_are_valid() {
        // Test that all scenario names from list command resolve correctly
        let scenario_names = vec![
            "baseline_good",
            "bonding_asymmetric",
            "mobile_handover",
            "degrading_network",
            "nr_to_lte_handover",
            "nr_mmwave_mobility",
            "nr_network_slicing",
            "nr_carrier_aggregation_test",
            "nr_beamforming_interference",
        ];

        for scenario_name in scenario_names {
            let result = resolve_scenario(scenario_name);
            assert!(
                result.is_ok(),
                "Scenario '{}' should be valid",
                scenario_name
            );

            let scenario = result.unwrap();
            assert_eq!(scenario.name, scenario_name);
            assert!(
                !scenario.links.is_empty(),
                "Scenario '{}' should have links",
                scenario_name
            );
        }
    }

    #[test]
    fn test_preset_scenario_consistency() {
        // Test that preset mappings are consistent
        let good_via_preset = resolve_preset(Some("good".to_string())).unwrap();
        let good_via_scenario = resolve_scenario("baseline_good").unwrap();
        assert_eq!(good_via_preset.name, good_via_scenario.name);

        let poor_via_preset = resolve_preset(Some("poor".to_string())).unwrap();
        let poor_via_scenario = resolve_scenario("degrading_network").unwrap();
        assert_eq!(poor_via_preset.name, poor_via_scenario.name);

        let lte_via_preset = resolve_preset(Some("lte".to_string())).unwrap();
        let lte_via_scenario = resolve_scenario("mobile_handover").unwrap();
        assert_eq!(lte_via_preset.name, lte_via_scenario.name);

        let bonding_via_preset = resolve_preset(Some("bonding".to_string())).unwrap();
        let bonding_via_scenario = resolve_scenario("bonding_asymmetric").unwrap();
        assert_eq!(bonding_via_preset.name, bonding_via_scenario.name);
    }

    #[tokio::test]
    async fn test_cmd_list_executes() {
        // Test that cmd_list doesn't panic and completes successfully
        let result = cmd_list().await;
        assert!(result.is_ok(), "cmd_list should execute without error");
    }

    #[tokio::test]
    async fn test_cmd_stats_placeholder() {
        // Test that cmd_stats executes (even though it's a placeholder)
        let result = cmd_stats(1).await;
        assert!(result.is_ok(), "cmd_stats should execute without error");
    }

    #[test]
    fn test_error_messages() {
        // Test that error messages are descriptive
        let invalid_preset_result = resolve_preset(Some("invalid_preset".to_string()));
        assert!(invalid_preset_result.is_err());
        let err_msg = format!("{}", invalid_preset_result.unwrap_err());
        assert!(err_msg.contains("invalid_preset"));

        let invalid_scenario_result = resolve_scenario("invalid_scenario");
        assert!(invalid_scenario_result.is_err());
        let err_msg = format!("{}", invalid_scenario_result.unwrap_err());
        assert!(err_msg.contains("invalid_scenario"));
    }
}
