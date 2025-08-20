//! Integration tests for bench-cli command-line interface
//!
//! This module organizes CLI tests into focused areas:
//! - general_cli: Help, version, error handling, and cross-cutting concerns
//! - list_command: Tests for the 'list' command functionality
//! - up_command: Tests for the 'up' command functionality  
//! - run_command: Tests for the 'run' command functionality
//! - stats_command: Tests for the 'stats' command functionality

mod general_cli;
mod list_command;
mod run_command;
mod stats_command;
mod up_command;
