//! Network testbench CLI tool
//!
//! This tool provides command-line access to the netns-testbench functionality
//! for running network scenarios, managing links, and observing metrics.

use anyhow::Result;
use clap::{Parser, Subcommand};
use netns_testbench::{NetworkOrchestrator, TestScenario};
use scenarios::Presets;
use std::time::Duration;
use tokio::signal;
use tokio::time::sleep;
use tracing::{error, info, Level};

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

async fn cmd_up(links: u8, preset: Option<String>, duration: u64, rx_port: u16) -> Result<()> {
    info!("Bringing up {} links for {} seconds", links, duration);

    let mut orchestrator = NetworkOrchestrator::new(42).await?;

    // Determine which scenario to use
    let scenario = match preset.as_deref() {
        Some("good") => TestScenario::baseline_good(),
        Some("poor") => TestScenario::degrading_network(),
        Some("lte") => TestScenario::mobile_handover(),
        Some("bonding") => TestScenario::bonding_asymmetric(),
        Some(name) => {
            error!("Unknown preset: {}", name);
            std::process::exit(1);
        }
        None => TestScenario::baseline_good(),
    };

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

async fn cmd_run(scenario: String, rx_port: u16) -> Result<()> {
    info!("Running scenario: {}", scenario);

    // Try to load as preset first
    let test_scenario = match scenario.as_str() {
        "baseline_good" => TestScenario::baseline_good(),
        "bonding_asymmetric" => TestScenario::bonding_asymmetric(),
        "mobile_handover" => TestScenario::mobile_handover(),
        "degrading_network" => TestScenario::degrading_network(),
        "nr_to_lte_handover" => TestScenario::nr_to_lte_handover(),
        "nr_mmwave_mobility" => TestScenario::nr_mmwave_mobility(),
        "nr_network_slicing" => TestScenario::nr_network_slicing(),
        "nr_carrier_aggregation_test" => TestScenario::nr_carrier_aggregation_test(),
        "nr_beamforming_interference" => TestScenario::nr_beamforming_interference(),
        _ => {
            // Try to load from file
            error!("Scenario file loading not yet implemented");
            std::process::exit(1);
        }
    };

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

async fn cmd_list() -> Result<()> {
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

async fn cmd_stats(_interval: u64) -> Result<()> {
    // TODO: Implement live statistics display
    println!("Live statistics display not yet implemented");
    println!("This would show:");
    println!("  - Active links and their current parameters");
    println!("  - Packet counts and rates");
    println!("  - Current impairment settings");
    println!("  - Schedule progress");

    Ok(())
}
