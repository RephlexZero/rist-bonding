//! Network emulator CLI
//!
//! Provides a command-line interface for managing network emulation scenarios.

use anyhow::Result;
use ristsmart_netem::{builder::EmulatorBuilder, types::*};
use std::path::PathBuf;
use std::time::Duration;
use tokio::time;
use tracing::{info, warn};

#[derive(Debug)]
enum Command {
    Up { scenario: PathBuf },
    Down { scenario: PathBuf },
    Run { scenario: PathBuf, duration: u64, metrics: Option<PathBuf> },
}

fn parse_args() -> Result<Command> {
    let args: Vec<String> = std::env::args().collect();
    
    if args.len() < 3 {
        eprintln!("Usage:");
        eprintln!("  {} up --scenario <path>", args[0]);
        eprintln!("  {} down --scenario <path>", args[0]);
        eprintln!("  {} run --scenario <path> --duration <seconds> [--metrics <path>]", args[0]);
        std::process::exit(1);
    }
    
    match args[1].as_str() {
        "up" => {
            if args.len() >= 4 && args[2] == "--scenario" {
                Ok(Command::Up { scenario: PathBuf::from(&args[3]) })
            } else {
                anyhow::bail!("Invalid arguments for 'up' command")
            }
        }
        "down" => {
            if args.len() >= 4 && args[2] == "--scenario" {
                Ok(Command::Down { scenario: PathBuf::from(&args[3]) })
            } else {
                anyhow::bail!("Invalid arguments for 'down' command")
            }
        }
        "run" => {
            let mut scenario = None;
            let mut duration = None;
            let mut metrics = None;
            
            let mut i = 2;
            while i < args.len() {
                match args[i].as_str() {
                    "--scenario" => {
                        if i + 1 < args.len() {
                            scenario = Some(PathBuf::from(&args[i + 1]));
                            i += 2;
                        } else {
                            anyhow::bail!("--scenario requires a value");
                        }
                    }
                    "--duration" => {
                        if i + 1 < args.len() {
                            duration = Some(args[i + 1].parse::<u64>()?);
                            i += 2;
                        } else {
                            anyhow::bail!("--duration requires a value");
                        }
                    }
                    "--metrics" => {
                        if i + 1 < args.len() {
                            metrics = Some(PathBuf::from(&args[i + 1]));
                            i += 2;
                        } else {
                            anyhow::bail!("--metrics requires a value");
                        }
                    }
                    _ => anyhow::bail!("Unknown argument: {}", args[i])
                }
            }
            
            match (scenario, duration) {
                (Some(s), Some(d)) => Ok(Command::Run { scenario: s, duration: d, metrics }),
                _ => anyhow::bail!("Both --scenario and --duration are required for 'run' command")
            }
        }
        _ => anyhow::bail!("Unknown command: {}", args[1])
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();
    
    let command = parse_args()?;
    
    match command {
        Command::Up { scenario } => {
            info!("Starting emulator with scenario: {}", scenario.display());
            
            let builder = EmulatorBuilder::from_file(&scenario).await?;
            let emulator = builder.build().await?;
            
            emulator.start().await?;
            
            info!("Emulator started. Press Ctrl+C to stop.");
            
            // Wait for Ctrl+C
            tokio::signal::ctrl_c().await?;
            
            info!("Stopping emulator...");
            emulator.teardown().await?;
            info!("Emulator stopped.");
        }
        
        Command::Down { scenario } => {
            warn!("Down command not fully implemented - emulator should auto-cleanup on exit");
            // In a full implementation, you might maintain a registry of running emulators
            // and be able to stop them by scenario name
        }
        
        Command::Run { scenario, duration, metrics } => {
            info!("Running emulator for {} seconds with scenario: {}", duration, scenario.display());
            
            let builder = EmulatorBuilder::from_file(&scenario).await?;
            let emulator = builder.build().await?;
            
            emulator.start().await?;
            
            // Set up metrics collection if requested
            if let Some(metrics_path) = metrics {
                info!("Writing metrics to: {}", metrics_path.display());
                
                let mut metrics_writer = ristsmart_netem::metrics::MetricsWriter::new_file(&metrics_path)?;
                let emulator_clone = std::sync::Arc::new(emulator);
                let emulator_for_metrics = emulator_clone.clone();
                
                // Spawn metrics collection task
                let metrics_task = tokio::spawn(async move {
                    let mut interval = time::interval(Duration::from_secs(1));
                    
                    for _ in 0..duration {
                        interval.tick().await;
                        
                        match emulator_for_metrics.metrics().await {
                            Ok(snapshot) => {
                                if let Err(e) = metrics_writer.write_snapshot(&snapshot) {
                                    warn!("Failed to write metrics: {}", e);
                                }
                            }
                            Err(e) => {
                                warn!("Failed to collect metrics: {}", e);
                            }
                        }
                    }
                });
                
                // Wait for duration
                time::sleep(Duration::from_secs(duration)).await;
                
                // Stop metrics collection
                metrics_task.abort();
                
                // Cleanup
                if let Err(e) = emulator_clone.as_ref().teardown().await {
                    warn!("Error during teardown: {}", e);
                }
            } else {
                // Just wait for duration
                time::sleep(Duration::from_secs(duration)).await;
                
                if let Err(e) = emulator.teardown().await {
                    warn!("Error during teardown: {}", e);
                }
            }
            
            info!("Emulator run complete.");
        }
    }
    
    Ok(())
}
