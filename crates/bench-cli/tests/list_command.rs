//! Tests for the 'list' command
//!
//! Verifies that the list command correctly displays available scenarios
//! and presets with proper formatting and content.

use assert_cmd::Command;
use predicates::prelude::*;

/// Helper function to create a command instance for the bench-cli binary
fn cli_command() -> Command {
    Command::cargo_bin("bench-cli").expect("Failed to find bench-cli binary")
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
fn test_cli_list_help() {
    let mut cmd = cli_command();
    cmd.args(&["list", "--help"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("List available scenarios"))
        .stdout(predicate::str::contains("--verbose"));
}
