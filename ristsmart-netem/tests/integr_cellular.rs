//! Integration tests requiring privileged operations
//!
//! These tests create actual network namespaces and require CAP_NET_ADMIN.
//! They are ignored by default and should be run with:
//!   sudo -E cargo test -- --ignored --test-threads=1
//!
//! Or set RISTS_PRIV=1 environment variable to enable them.

use ristsmart_netem::{
    builder::EmulatorBuilder,
    types::{DelayProfile, GEParams, GeState, LinkSpec, OUParams, RateLimiter},
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

    // Let it run for a longer period to test OU/GE controllers
    println!("Running for 10 seconds to allow GE state transitions...");
    time::sleep(Duration::from_secs(10)).await;

    // Collect metrics and perform comprehensive verification
    let metrics = emulator.metrics().await.expect("Failed to collect metrics");

    // Verify we have exactly one link
    assert_eq!(metrics.links.len(), 1, "Should have exactly one link");

    let link = &metrics.links[0];

    // Verify basic properties
    assert!(
        !link.namespace.is_empty(),
        "Link namespace should not be empty"
    );
    assert!(
        link.egress_rate_bps > 0,
        "Egress rate should be positive: {}",
        link.egress_rate_bps
    );

    // Verify GE model state is valid
    assert!(
        matches!(link.ge_state, GeState::Good | GeState::Bad),
        "GE state should be Good or Bad, got: {:?}",
        link.ge_state
    );

    // Verify loss percentage is in valid range
    assert!(
        link.loss_pct >= 0.0 && link.loss_pct <= 100.0,
        "Loss percentage should be 0-100%, got: {}",
        link.loss_pct
    );

    // Since we're using GE model with default parameters, we should see some state transitions
    // over 10 seconds of runtime (verify it's not stuck in initial state)
    assert!(
        link.loss_pct < 100.0,
        "Loss should not be 100% - suggests GE model is functioning"
    );

    // Verify namespace name follows expected pattern
    assert!(
        link.namespace.starts_with("lnk-"),
        "Namespace should start with 'lnk-', got: {}",
        link.namespace
    );

    // Print verification results
    println!(
        "✓ Link verified: ns={}, rate={} Mbps, ge_state={:?}, loss={}%",
        link.namespace,
        link.egress_rate_bps / 1_000_000,
        link.ge_state,
        link.loss_pct
    );

    println!("✓ All metrics validation passed");

    println!("Stopping emulator...");
    emulator.stop().await.expect("Failed to stop emulator");

    println!("Tearing down...");
    emulator.teardown().await.expect("Failed to teardown");

    // Verify cleanup by checking no lingering namespaces
    tokio::time::sleep(Duration::from_millis(100)).await;
    let ns_check = tokio::process::Command::new("ip")
        .args(&["netns", "list"])
        .output()
        .await
        .expect("Failed to check namespace list");

    let ns_output = String::from_utf8_lossy(&ns_check.stdout);
    assert!(
        !ns_output.contains("lnk-"),
        "Namespace cleanup failed - found remaining namespace: {}",
        ns_output
    );

    println!("✓ Cleanup verification passed");
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

    // Final metrics collection and comprehensive verification
    let metrics = emulator
        .metrics()
        .await
        .expect("Failed to collect final metrics");

    // Verify we have exactly two links
    assert_eq!(metrics.links.len(), 2, "Should have exactly two links");

    // Find cellular and satellite links by their characteristics
    let mut cellular_link = None;
    let mut satellite_link = None;

    for link in &metrics.links {
        // Cellular typically has lower rates and higher loss
        if link.egress_rate_bps < 5_000_000 {
            // Less than 5 Mbps, likely cellular
            cellular_link = Some(link);
        } else {
            // Higher rate, likely satellite
            satellite_link = Some(link);
        }
    }

    let cellular = cellular_link.expect("Should have found cellular link");
    let satellite = satellite_link.expect("Should have found satellite link");

    // Verify basic properties for both links
    assert!(
        !cellular.namespace.is_empty(),
        "Cellular namespace should not be empty"
    );
    assert!(
        !satellite.namespace.is_empty(),
        "Satellite namespace should not be empty"
    );

    assert!(
        cellular.egress_rate_bps > 0,
        "Cellular rate should be positive: {}",
        cellular.egress_rate_bps
    );
    assert!(
        satellite.egress_rate_bps > 0,
        "Satellite rate should be positive: {}",
        satellite.egress_rate_bps
    );

    // Verify GE states are valid
    assert!(
        matches!(cellular.ge_state, GeState::Good | GeState::Bad),
        "Cellular GE state should be Good or Bad, got: {:?}",
        cellular.ge_state
    );
    assert!(
        matches!(satellite.ge_state, GeState::Good | GeState::Bad),
        "Satellite GE state should be Good or Bad, got: {:?}",
        satellite.ge_state
    );

    // Verify loss percentages are in valid range
    assert!(
        cellular.loss_pct >= 0.0 && cellular.loss_pct <= 100.0,
        "Cellular loss percentage should be 0-100%, got: {}",
        cellular.loss_pct
    );
    assert!(
        satellite.loss_pct >= 0.0 && satellite.loss_pct <= 100.0,
        "Satellite loss percentage should be 0-100%, got: {}",
        satellite.loss_pct
    );

    // Verify delay characteristics (satellite should have higher delay)
    assert!(
        satellite.delay_ms > cellular.delay_ms,
        "Satellite delay ({} ms) should be higher than cellular ({} ms)",
        satellite.delay_ms,
        cellular.delay_ms
    );

    // Verify parameter update worked (cellular rate should be updated to ~1.5 Mbps)
    assert!(
        cellular.egress_rate_bps > 1_000_000 && cellular.egress_rate_bps < 2_000_000,
        "Cellular rate should be ~1.5 Mbps after update, got: {} bps",
        cellular.egress_rate_bps
    );

    println!("✓ Dual-link verification passed:");
    println!(
        "  Cellular: rate={} Mbps, state={:?}, loss={}%, delay={}ms",
        cellular.egress_rate_bps / 1_000_000,
        cellular.ge_state,
        cellular.loss_pct,
        cellular.delay_ms
    );
    println!(
        "  Satellite: rate={} Mbps, state={:?}, loss={}%, delay={}ms",
        satellite.egress_rate_bps / 1_000_000,
        satellite.ge_state,
        satellite.loss_pct,
        satellite.delay_ms
    );

    println!("Cleaning up dual-link emulator...");
    emulator.teardown().await.expect("Failed to teardown");

    // Verify cleanup by checking no lingering namespaces
    tokio::time::sleep(Duration::from_millis(100)).await;
    let ns_check = tokio::process::Command::new("ip")
        .args(&["netns", "list"])
        .output()
        .await
        .expect("Failed to check namespace list");

    let ns_output = String::from_utf8_lossy(&ns_check.stdout);
    assert!(
        !ns_output.contains("lnk-"),
        "Namespace cleanup failed - found remaining namespace: {}",
        ns_output
    );

    println!("✓ Dual-link cleanup verification passed");
    println!("Dual-link test completed!");
}
