//! Integration tests for bench-cli command-line interface
//!
//! These tests verify that the CLI tool works correctly with various command
//! combinations, error handling, and expected output formats.

use assert_cmd::Command;
use predicates::prelude::*;
use std::time::Duration;

/// Helper function to create a command instance for the bench-cli binary
fn cli_command() -> Command {
    Command::cargo_bin("bench-cli").expect("Failed to find bench-cli binary")
}

#[test]
fn test_cli_help() {
    let mut cmd = cli_command();
    cmd.arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Network testbench CLI tool"))
        .stdout(predicate::str::contains("USAGE:"))
        .stdout(predicate::str::contains("COMMANDS:"))
        .stdout(predicate::str::contains("up"))
        .stdout(predicate::str::contains("run"))
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("stats"));
}

#[test]
fn test_cli_version() {
    let mut cmd = cli_command();
    cmd.arg("--version");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("bench-cli"));
}

#[test]
fn test_cli_invalid_command() {
    let mut cmd = cli_command();
    cmd.arg("invalid-command");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("error: unrecognized subcommand"));
}

#[test]
fn test_cli_list_command() {
    let mut cmd = cli_command();
    cmd.arg("list");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Available scenarios:"))
        .stdout(predicate::str::contains("Basic scenarios:"))
        .stdout(predicate::str::contains("Built-in presets:"))
        .stdout(predicate::str::contains("good"))
        .stdout(predicate::str::contains("poor"))
        .stdout(predicate::str::contains("lte"))
        .stdout(predicate::str::contains("bonding"));
}

#[test]
fn test_cli_list_with_verbose() {
    let mut cmd = cli_command();
    cmd.args(&["--verbose", "list"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Available scenarios:"));
}

#[test]
fn test_cli_up_help() {
    let mut cmd = cli_command();
    cmd.args(&["up", "--help"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Bring up network links"))
        .stdout(predicate::str::contains("--links"))
        .stdout(predicate::str::contains("--preset"))
        .stdout(predicate::str::contains("--duration"))
        .stdout(predicate::str::contains("--rx-port"));
}

#[test]
fn test_cli_up_invalid_preset() {
    let mut cmd = cli_command();
    cmd.args(&["up", "--preset", "invalid_preset", "--duration", "1"]);

    // This should fail with an error about unknown preset
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Unknown preset"));
}

#[test]
fn test_cli_up_with_timeout() {
    // Test that the up command can be started and handles short durations
    // Note: This test may fail on systems without network permissions
    let mut cmd = cli_command();
    cmd.args(&[
        "up",
        "--preset",
        "good",
        "--duration",
        "1", // Very short duration
        "--links",
        "1",
        "--rx-port",
        "7001", // Non-default port to avoid conflicts
    ]);

    // Set a timeout to prevent hanging
    cmd.timeout(Duration::from_secs(10));

    let output = cmd.output().expect("Failed to execute command");

    if output.status.success() {
        // If successful, should see startup and shutdown messages
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("Bringing up") || stdout.contains("shut down"));
    } else {
        // If failed due to permissions, should be a clean error
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("permission")
                || stderr.contains("Operation not permitted")
                || stderr.contains("capability")
        );
    }
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
        "Scenario file loading not yet implemented",
    ));
}

#[test]
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

#[test]
fn test_cli_stats_command() {
    let mut cmd = cli_command();
    cmd.arg("stats");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "Live statistics display not yet implemented",
        ))
        .stdout(predicate::str::contains("This would show:"))
        .stdout(predicate::str::contains("Active links"))
        .stdout(predicate::str::contains("Packet counts"));
}

#[test]
fn test_cli_stats_with_interval() {
    let mut cmd = cli_command();
    cmd.args(&["stats", "--interval", "5"]);

    cmd.assert().success().stdout(predicate::str::contains(
        "Live statistics display not yet implemented",
    ));
}

#[test]
fn test_cli_stats_help() {
    let mut cmd = cli_command();
    cmd.args(&["stats", "--help"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Show live statistics"))
        .stdout(predicate::str::contains("--interval"));
}

#[test]
fn test_cli_up_parameter_validation() {
    // Test various parameter combinations for the up command

    // Test with zero links (should be valid as it's u8, but might be handled by logic)
    let mut cmd = cli_command();
    cmd.args(&["up", "--links", "0", "--duration", "1"]);
    cmd.timeout(Duration::from_secs(5));

    let output = cmd.output().expect("Failed to execute command");
    // The command should handle 0 links gracefully (either succeed with no-op or fail cleanly)
    if !output.status.success() {
        // If it fails, it should be a clean failure, not a panic
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!stderr.is_empty());
    }

    // Test with very large number of links
    let mut cmd = cli_command();
    cmd.args(&["up", "--links", "255", "--duration", "1"]);
    cmd.timeout(Duration::from_secs(5));

    let output = cmd.output().expect("Failed to execute command");
    // Should either succeed or fail gracefully
    assert!(output.status.code().unwrap_or(1) <= 1); // Either success (0) or clean failure (1)

    // Test with invalid port numbers
    let mut cmd = cli_command();
    cmd.args(&["up", "--rx-port", "0", "--duration", "1"]);

    // Port 0 might be invalid for binding but should be handled gracefully
    cmd.timeout(Duration::from_secs(5));
    let output = cmd.output().expect("Failed to execute command");
    if !output.status.success() {
        // Should fail with a meaningful error, not panic
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!stderr.is_empty());
    }
}

#[test]
fn test_cli_verbose_flag_positioning() {
    // Test that --verbose flag works in different positions

    // Before subcommand
    let mut cmd = cli_command();
    cmd.args(&["--verbose", "list"]);
    cmd.assert().success();

    // After subcommand (should still work due to global flag)
    let mut cmd = cli_command();
    cmd.args(&["list", "--verbose"]);
    cmd.assert().success();
}

#[test]
fn test_cli_error_handling_and_exit_codes() {
    // Test that the CLI returns appropriate exit codes

    // Invalid command should return non-zero
    let mut cmd = cli_command();
    cmd.arg("invalid");
    cmd.assert().failure().code(2); // clap typically returns 2 for usage errors

    // Help should return 0
    let mut cmd = cli_command();
    cmd.arg("--help");
    cmd.assert().success().code(0);

    // Version should return 0
    let mut cmd = cli_command();
    cmd.arg("--version");
    cmd.assert().success().code(0);
}

#[test]
fn test_cli_concurrent_execution() {
    // Test that multiple CLI instances can be invoked concurrently without interfering
    use std::thread;

    let handles: Vec<_> = (0..3)
        .map(|_| {
            thread::spawn(move || {
                let mut cmd = cli_command();
                cmd.args(&["list"]);

                let output = cmd.assert().success();
                output.stdout(predicate::str::contains("Available scenarios:"));
            })
        })
        .collect();

    // Wait for all threads to complete
    for handle in handles {
        handle.join().expect("Thread should complete successfully");
    }
}

#[test]
fn test_cli_signal_handling_simulation() {
    // Test that commands with timeouts complete within reasonable time
    // This simulates interrupt handling without actually sending signals

    let mut cmd = cli_command();
    cmd.args(&["up", "--duration", "2", "--rx-port", "7004"]);
    cmd.timeout(Duration::from_secs(15)); // Allow enough time for 2-second duration + overhead

    let output = cmd.output().expect("Failed to execute command");

    if output.status.success() {
        // If successful, should complete in reasonable time (not hang)
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("Testbench shut down successfully")
                || stdout.contains("Duration completed")
        );
    } else {
        // If failed, should be clean failure, not timeout
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!stderr.is_empty());
    }
}
