//! Network testbench CLI tool
//!
//! This tool provides command-line access to the netns-testbench functionality
//! for running network scenarios, managing links, and observing metrics.

mod commands;

use anyhow::Result;
use clap::{Parser, Subcommand};
use commands::{cmd_list, cmd_run, cmd_stats, cmd_up};
use tracing::Level;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Bring up network links with a preset or scenario
    Up {
        /// Number of links to create
        #[arg(long, default_value_t = 1)]
        links: u8,

        /// Preset name (good, poor, lte, etc.)
        #[arg(long)]
        preset: Option<String>,

        /// Duration to run (seconds)
        #[arg(long, default_value_t = 60)]
        duration: u64,

        /// RX port for scenarios
        #[arg(long, default_value_t = 7000)]
        rx_port: u16,
    },

    /// Run a specific scenario from file or preset
    Run {
        /// Scenario file path (JSON) or preset name
        scenario: String,

        /// RX port for scenarios
        #[arg(long, default_value_t = 7000)]
        rx_port: u16,
    },

    /// List available scenarios and presets
    List,

    /// Show live statistics
    Stats {
        /// Update interval in seconds
        #[arg(long, default_value_t = 1)]
        interval: u64,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize tracing
    let level = if cli.verbose {
        Level::DEBUG
    } else {
        Level::INFO
    };
    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_target(false)
        .init();

    match cli.command {
        Commands::Up {
            links,
            preset,
            duration,
            rx_port,
        } => {
            cmd_up(links, preset, duration, rx_port).await?;
        }
        Commands::Run { scenario, rx_port } => {
            cmd_run(scenario, rx_port).await?;
        }
        Commands::List => {
            cmd_list().await?;
        }
        Commands::Stats { interval } => {
            cmd_stats(interval).await?;
        }
    }

    Ok(())
}
