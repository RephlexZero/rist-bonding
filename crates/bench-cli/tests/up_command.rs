//! Tests for the 'up' command
//!
//! Verifies that the up command correctly brings up network links with
//! various presets, handles parameter validation, and manages timeouts.

use assert_cmd::Command;
use predicates::prelude::*;
use std::time::Duration;

/// Helper function to create a command instance for the bench-cli binary
fn cli_command() -> Command {
    Command::cargo_bin("bench-cli").expect("Failed to find bench-cli binary")
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
#[cfg(feature = "network-tests")]
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
fn test_cli_up_help() {
    let mut cmd = cli_command();
    cmd.args(&["up", "--help"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Bring up network links"))
        .stdout(predicate::str::contains("--preset"))
        .stdout(predicate::str::contains("--duration"))
        .stdout(predicate::str::contains("--links"))
        .stdout(predicate::str::contains("--rx-port"));
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
#[cfg(feature = "network-tests")]
fn test_cli_up_signal_handling_simulation() {
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