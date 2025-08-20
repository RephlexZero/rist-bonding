//! General CLI tests covering help, version, error handling, and cross-cutting concerns
//!
//! Tests basic CLI functionality that doesn't fit into specific command categories,
//! including global flags, error codes, concurrent execution, and overall CLI behavior.

use assert_cmd::Command;
use predicates::prelude::*;

/// Helper function to create a command instance for the bench-cli binary
fn cli_command() -> Command {
    Command::cargo_bin("bench-cli").expect("Failed to find bench-cli binary")
}

#[test]
fn test_cli_help_and_version() {
    // Combined test for help and version to reduce clap auto-generated output redundancy
    let mut cmd = cli_command();
    cmd.arg("--help");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "CLI tool for running RIST network testbench scenarios",
        ))
        .stdout(predicate::str::contains("Commands:"));

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
fn test_subcommand_help() {
    // Combined test for subcommand help to reduce redundant clap output testing
    let subcommands = vec![
        ("up", "Bring up network links"),
        ("run", "Run a specific scenario"),
        ("stats", "Show live statistics"),
    ];

    for (cmd, description) in subcommands {
        let mut command = cli_command();
        command.args(&[cmd, "--help"]);
        command
            .assert()
            .success()
            .stdout(predicate::str::contains(description));
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
