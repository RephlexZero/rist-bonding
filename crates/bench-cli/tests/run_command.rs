//! Tests for the 'run' command
//!
//! Verifies that the run command correctly executes scenarios,
//! handles valid/invalid scenario names, and manages execution timeouts.

use assert_cmd::Command;
use predicates::prelude::*;

#[cfg(feature = "network-tests")]
use std::time::Duration;

/// Helper function to create a command instance for the bench-cli binary
fn cli_command() -> Command {
    Command::cargo_bin("bench-cli").expect("Failed to find bench-cli binary")
}

#[test]
fn test_cli_run_help() {
    let mut cmd = cli_command();
    cmd.args(&["run", "--help"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Run a specific scenario"))
        .stdout(predicate::str::contains("scenario"))
        .stdout(predicate::str::contains("--rx-port"));
}

#[test]
#[cfg(feature = "network-tests")]
fn test_cli_run_valid_scenario() {
    // Test running a valid built-in scenario with very short duration
    let mut cmd = cli_command();
    cmd.args(&["run", "baseline_good", "--rx-port", "7002"]);

    cmd.timeout(Duration::from_secs(10));

    let output = cmd.output().expect("Failed to execute command");

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("Running scenario: baseline_good"));
    } else {
        // Should fail gracefully with permission error
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("permission")
                || stderr.contains("Operation not permitted")
                || stderr.contains("capability")
        );
    }
}

#[test]
fn test_cli_run_invalid_scenario() {
    let mut cmd = cli_command();
    cmd.args(&["run", "nonexistent_scenario"]);

    cmd.assert().failure().stderr(predicate::str::contains(
        "not found. File loading not yet implemented",
    ));
}

#[test]
#[cfg(feature = "network-tests")]
fn test_cli_run_known_scenarios() {
    let scenarios = vec![
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

    for scenario in scenarios {
        let mut cmd = cli_command();
        cmd.args(&["run", scenario, "--rx-port", "7003"]);
        cmd.timeout(Duration::from_secs(5));

        let output = cmd.output().expect("Failed to execute command");

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(stdout.contains(&format!("Running scenario: {}", scenario)));
        } else {
            // Should be permission-related failure, not unknown scenario
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                stderr.contains("permission")
                    || stderr.contains("Operation not permitted")
                    || stderr.contains("capability")
            );
        }
    }
}
