//! Tests for the 'stats' command
//!
//! Verifies that the stats command displays appropriate messages for
//! not-yet-implemented features and handles intervals correctly.

use assert_cmd::Command;
use predicates::prelude::*;

/// Helper function to create a command instance for the bench-cli binary
fn cli_command() -> Command {
    Command::cargo_bin("bench-cli").expect("Failed to find bench-cli binary")
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
