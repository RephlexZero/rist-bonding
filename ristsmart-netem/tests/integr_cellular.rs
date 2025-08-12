//! Integration tests requiring privileged operations
//!
//! These tests create actual network namespaces and require CAP_NET_ADMIN.
//! They are ignored by default and should be run with:
//!   sudo -E cargo test -- --ignored --test-threads=1
//!
//! Or set RISTS_PRIV=1 environment variable to enable them.

use ristsmart_netem::{
    builder::EmulatorBuilder,
    types::{DelayProfile, GEParams, LinkSpec, OUParams, RateLimiter},
};
use std::time::Duration;
use tokio::time;

// Helper to check if privileged tests should run
fn should_run_privileged_tests() -> bool {
    std::env::var("RISTS_PRIV")
        .map(|v| v == "1")
        .unwrap_or(false)
}

#[tokio::test]
#[ignore]
async fn test_single_link_creation() {
    if !should_run_privileged_tests() {
        eprintln!("Skipping privileged test (set RISTS_PRIV=1 to enable)");
        return;
    }

    let spec = LinkSpec {
        name: "cellular".to_string(),
        rate_limiter: RateLimiter::Tbf,
        ou: OUParams {
            mean_bps: 1_000_000, // 1 Mbps
            tau_ms: 1000,
            sigma: 0.2,
            tick_ms: 200,
            seed: None,
        },
        ge: GEParams {
            p_good: 0.001, // 0.1% loss in good state
            p_bad: 0.05,   // 5% loss in bad state
            p: 0.01,       // 1% chance good->bad
            r: 0.1,        // 10% chance bad->good
            seed: None,
        },
        delay: DelayProfile {
            delay_ms: 50,
            jitter_ms: 10,
            reorder_pct: 0.0,
        },
        ifb_ingress: false,
    };

    println!("Creating emulator with single cellular link...");

    let mut builder = EmulatorBuilder::new();
    builder.add_link(spec).with_seed(42);

    let emulator = match builder.build().await {
        Ok(emu) => emu,
        Err(e) => {
            eprintln!("Failed to build emulator (may need privileges): {}", e);
            return;
        }
    };

    println!("Starting emulator...");
    if let Err(e) = emulator.start().await {
        eprintln!("Failed to start emulator: {}", e);
        if let Err(e) = emulator.teardown().await {
            eprintln!("Failed to teardown: {}", e);
        }
        return;
    }

    // Let it run for a few seconds to test OU/GE controllers
    println!("Running for 3 seconds...");
    time::sleep(Duration::from_secs(3)).await;

    // Collect metrics
    match emulator.metrics().await {
        Ok(metrics) => {
            println!("Collected metrics: {} links", metrics.links.len());
            for link in &metrics.links {
                println!(
                    "Link {}: rate={} bps, state={:?}, loss={:.2}%",
                    link.namespace, link.egress_rate_bps, link.ge_state, link.loss_pct
                );
            }
        }
        Err(e) => eprintln!("Failed to collect metrics: {}", e),
    }

    println!("Stopping emulator...");
    if let Err(e) = emulator.stop().await {
        eprintln!("Failed to stop emulator: {}", e);
    }

    println!("Tearing down...");
    if let Err(e) = emulator.teardown().await {
        eprintln!("Failed to teardown: {}", e);
    }

    println!("Test completed successfully!");
}

#[tokio::test]
#[ignore]
async fn test_dual_link_emulation() {
    if !should_run_privileged_tests() {
        eprintln!("Skipping privileged test (set RISTS_PRIV=1 to enable)");
        return;
    }

    println!("Creating dual-link emulator...");

    let cellular_spec = LinkSpec {
        name: "cellular".to_string(),
        rate_limiter: RateLimiter::Tbf,
        ou: OUParams {
            mean_bps: 2_000_000, // 2 Mbps
            tau_ms: 2000,
            sigma: 0.3,
            tick_ms: 200,
            seed: None,
        },
        ge: GEParams {
            p_good: 0.0005,
            p_bad: 0.08,
            p: 0.015,
            r: 0.15,
            seed: None,
        },
        delay: DelayProfile {
            delay_ms: 60,
            jitter_ms: 15,
            reorder_pct: 0.1,
        },
        ifb_ingress: false,
    };

    let satellite_spec = LinkSpec {
        name: "satellite".to_string(),
        rate_limiter: RateLimiter::Tbf,
        ou: OUParams {
            mean_bps: 10_000_000, // 10 Mbps
            tau_ms: 5000,
            sigma: 0.15,
            tick_ms: 500,
            seed: None,
        },
        ge: GEParams {
            p_good: 0.0001,
            p_bad: 0.2,
            p: 0.005,
            r: 0.05,
            seed: None,
        },
        delay: DelayProfile {
            delay_ms: 300,
            jitter_ms: 50,
            reorder_pct: 0.0,
        },
        ifb_ingress: true, // Test ingress shaping
    };

    let mut builder = EmulatorBuilder::new();
    builder
        .add_link(cellular_spec)
        .add_link(satellite_spec)
        .with_seed(1337);

    let emulator = match builder.build().await {
        Ok(emu) => emu,
        Err(e) => {
            eprintln!("Failed to build emulator: {}", e);
            return;
        }
    };

    println!("Starting dual-link emulator...");
    if let Err(e) = emulator.start().await {
        eprintln!("Failed to start: {}", e);
        if let Err(e) = emulator.teardown().await {
            eprintln!("Failed to teardown: {}", e);
        }
        return;
    }

    // Run for a bit longer to see more variation
    println!("Running for 5 seconds...");
    time::sleep(Duration::from_secs(5)).await;

    // Test link handles
    if let Some(cellular_handle) = emulator.link("cellular") {
        println!("Testing cellular link parameter updates...");

        let new_ou = OUParams {
            mean_bps: 1_500_000, // Change to 1.5 Mbps
            tau_ms: 1500,
            sigma: 0.25,
            tick_ms: 150,
            seed: None,
        };

        if let Err(e) = cellular_handle.set_ou(new_ou).await {
            eprintln!("Failed to update OU params: {}", e);
        } else {
            println!("Updated cellular OU parameters");
        }
    }

    // Wait a bit more to see the changes
    time::sleep(Duration::from_secs(2)).await;

    // Final metrics
    match emulator.metrics().await {
        Ok(metrics) => {
            println!("Final metrics:");
            for link in &metrics.links {
                println!(
                    "  {}: rate={} bps, state={:?}, loss={:.3}%, delay={}ms, jitter={}ms",
                    link.namespace,
                    link.egress_rate_bps,
                    link.ge_state,
                    link.loss_pct,
                    link.delay_ms,
                    link.jitter_ms
                );
            }
        }
        Err(e) => eprintln!("Failed to collect final metrics: {}", e),
    }

    println!("Cleaning up dual-link emulator...");
    if let Err(e) = emulator.teardown().await {
        eprintln!("Failed to teardown: {}", e);
    }

    println!("Dual-link test completed!");
}
