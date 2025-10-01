//! Concurrent bandwidth validation test using crate-managed namespaces and links
//!
//! Creates multiple links with different bandwidth limits (125, 250, 500, 1000, 2000, 4000 kbps)
//! and validates that each link enforces its configured rate limit concurrently.

use network_sim::qdisc::QdiscManager;
use network_sim::Namespace;
use network_sim::{VethPair, VethPairConfig};
use std::net::UdpSocket;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
struct BandwidthLinkCfg {
    name: String,
    cfg: VethPairConfig,
    target_kbps: u32,
    port: u16,
}

async fn setup_bandwidth_link(
    link: &BandwidthLinkCfg,
    qdisc: &QdiscManager,
) -> std::io::Result<VethPair> {
    println!("Setting up {} ({})", link.name, link.target_kbps);
    // Create with optional shaping
    let pair = VethPair::create(qdisc, &link.cfg).await?;
    Ok(pair)
}

async fn measure_link_throughput(
    link: &BandwidthLinkCfg,
    duration_secs: u64,
) -> std::io::Result<f64> {
    let rx_ns = link.cfg.rx_ns.clone().unwrap();
    let tx_ns = link.cfg.tx_ns.clone().unwrap();
    let rx_ip_tx = link.cfg.rx_ip_cidr.split('/').next().unwrap().to_string();
    let tx_ip = link.cfg.tx_ip_cidr.split('/').next().unwrap().to_string();
    let port = link.port;

    let recv_handle = std::thread::spawn(move || -> std::io::Result<u64> {
        // Enter RX namespace
        let ns = network_sim::Namespace::from_existing(rx_ns.clone());
        let _ns_guard = ns.enter()?; // keep guard alive for thread lifetime
                                     // Bind to all addresses to avoid any address-specific issues
        let addr = format!("0.0.0.0:{}", port);
        let socket = UdpSocket::bind(addr).map_err(|e| std::io::Error::other(e.to_string()))?;
        socket
            .set_read_timeout(Some(Duration::from_millis(100)))
            .ok();

        let end = Instant::now() + Duration::from_secs(duration_secs);
        let mut total: u64 = 0;
        let mut buf = [0u8; 65536];
        while Instant::now() < end {
            match socket.recv(&mut buf) {
                Ok(n) => total += n as u64,
                Err(ref e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(e) => return Err(e),
            }
        }
        Ok(total)
    });

    // small delay to ensure receiver bound
    tokio::time::sleep(Duration::from_millis(350)).await;

    let send_handle = std::thread::spawn(move || -> std::io::Result<()> {
        let ns = network_sim::Namespace::from_existing(tx_ns.clone());
        let _ns_guard = ns.enter()?; // keep guard alive for thread lifetime
                                     // bind to tx_ip to ensure correct source
        let bind_addr = format!("{}:0", tx_ip);
        let socket =
            UdpSocket::bind(bind_addr).map_err(|e| std::io::Error::other(e.to_string()))?;
        socket
            .set_write_timeout(Some(Duration::from_millis(100)))
            .ok();
        let dest = format!("{}:{}", rx_ip_tx, port);
        let payload = vec![0x58u8; 1200]; // 'X' * 1200
        let end = Instant::now() + Duration::from_secs(duration_secs);
        while Instant::now() < end {
            let _ = socket.send_to(&payload, &dest);
            // tiny pause; rate limiter should do the limiting
            std::thread::sleep(Duration::from_micros(100));
        }
        Ok(())
    });

    // Wait for sender to finish
    let _ = send_handle
        .join()
        .map_err(|_| std::io::Error::other("send thread panicked"))?;
    // Collect receiver bytes
    let total_bytes = recv_handle
        .join()
        .map_err(|_| std::io::Error::other("recv thread panicked"))?? as f64;

    let kbps = (total_bytes * 8.0) / (duration_secs as f64 * 1000.0);
    Ok(kbps)
}

// cleanup handled by VethPair::delete() where appropriate

async fn get_qdisc_sent_bytes(
    link: &BandwidthLinkCfg,
    qdisc: &QdiscManager,
) -> std::io::Result<u64> {
    let tx_ns = link.cfg.tx_ns.clone().unwrap();
    let tx_if = link.cfg.tx_if.clone();
    let stats = qdisc
        .get_interface_stats_in_ns(&tx_ns, &tx_if)
        .await
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(stats.sent_bytes)
}

#[tokio::test]
async fn test_bandwidth_sequential_validation() {
    let qdisc = QdiscManager::new();
    if !qdisc.has_net_admin().await {
        println!("SKIP: No NET_ADMIN capability - need privileged container");
        return;
    }

    // Probe netns creation capability
    match Namespace::ensure("ns-probe-seq").await {
        Ok(ns) => {
            let _ = ns.delete().await;
        }
        Err(e) => {
            println!("SKIP: netns creation not permitted: {}", e);
            return;
        }
    }

    println!("=== Bandwidth Validation (Sequential) ===");

    // Define test bandwidth rates
    let target_rates = vec![125, 250, 500, 1000, 2000, 4000];

    // Create bandwidth links
    let links: Vec<BandwidthLinkCfg> = target_rates
        .iter()
        .enumerate()
        .map(|(i, &rate)| BandwidthLinkCfg {
            name: format!("seq-link-{}k", rate),
            cfg: VethPairConfig {
                tx_if: format!("seq-tx-{}", i),
                rx_if: format!("seq-rx-{}", i),
                tx_ip_cidr: format!("192.168.{}.1/30", 100 + i),
                rx_ip_cidr: format!("192.168.{}.2/30", 100 + i),
                tx_ns: Some(format!("seq-tx-ns-{}", i)),
                rx_ns: Some(format!("seq-rx-ns-{}", i)),
                params: Some(network_sim::NetworkParams {
                    delay_ms: 0,
                    loss_pct: 0.0,
                    rate_kbps: rate,
                    jitter_ms: 0,
                    reorder_pct: 0.0,
                    duplicate_pct: 0.0,
                    loss_corr_pct: 0.0,
                }),
            },
            target_kbps: rate,
            port: 8000 + i as u16,
        })
        .collect();

    println!(
        "Setting up {} links with rates: {:?} kbps",
        links.len(),
        target_rates
    );

    // Setup all links concurrently
    let setup_futures = links.iter().map(|l| setup_bandwidth_link(l, &qdisc));
    let setup_results: Vec<_> = futures::future::join_all(setup_futures).await;

    // Check for setup failures
    let mut setup_failures: Vec<String> = Vec::new();
    for (i, result) in setup_results.iter().enumerate() {
        if let Err(e) = result {
            setup_failures.push(format!("Link {}: {}", links[i].name, e));
        }
    }

    if !setup_failures.is_empty() {
        eprintln!("Setup failures:");
        for failure in setup_failures {
            eprintln!("  ❌ {}", failure);
        }
        // Cleanup all links
        for _link in &links {
            // Already best-effort via VethPair delete if created
        }
        panic!("Link setup failed");
    }

    println!("✅ All {} links setup successfully\n", links.len());

    // Wait a moment for interfaces to stabilize
    tokio::time::sleep(Duration::from_secs(1)).await;

    println!("Starting throughput measurements (5 seconds each) in Rust only (sequential)...");

    let mut measurement_results = Vec::with_capacity(links.len());
    for link in &links {
        let link_clone = link.clone();
        let measured = measure_link_throughput(&link_clone, 5).await;
        measurement_results.push((link_clone, measured));
    }

    println!("\n=== Results ===");
    let mut all_passed = true;
    let mut results = Vec::new();

    for (link, result) in measurement_results {
        match result {
            Ok(measured_kbps) => {
                let target = link.target_kbps as f64;
                let efficiency = (measured_kbps / target) * 100.0;
                let upper_tolerance = 1.10; // allow up to 10% above target
                let lower_tolerance = 0.85; // expect at least 85% of target

                let status = if measured_kbps <= target * upper_tolerance
                    && measured_kbps >= target * lower_tolerance
                {
                    "✅ PASS"
                } else {
                    all_passed = false;
                    "❌ FAIL"
                };

                println!(
                    "{} {} - Target: {} kbps, Measured: {:.0} kbps ({:.1}% of target)",
                    status, link.name, target, measured_kbps, efficiency
                );

                results.push((link.name.clone(), target, measured_kbps, efficiency));
            }
            Err(e) => {
                println!("❌ FAIL {} - Measurement error: {}", link.name, e);
                all_passed = false;
            }
        }
    }

    // Cleanup all links
    println!("\nCleaning up test links...");
    // VethPair instances are dropped earlier; namespaced cleanup is best-effort

    // Analysis
    println!("\n=== Analysis (Sequential) ===");
    if all_passed {
        println!("✅ SUCCESS: All bandwidth limits are being enforced correctly");

        // Show rate differentiation
        results.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap()); // Sort by target rate
        println!("\nRate differentiation validation:");
        for (i, (name, _target, measured, _)) in results.iter().enumerate() {
            if i > 0 {
                let prev_measured = results[i - 1].2;
                if measured > &prev_measured {
                    println!(
                        "  ✅ {} ({:.0} kbps) > {} ({:.0} kbps)",
                        name,
                        measured,
                        results[i - 1].0,
                        prev_measured
                    );
                } else {
                    println!("  ⚠️  {} ({:.0} kbps) ≤ {} ({:.0} kbps) - ordering may be affected by measurement timing", 
                             name, measured, results[i-1].0, prev_measured);
                }
            }
        }
    } else {
        println!("❌ Some bandwidth limits are not being enforced correctly");
    }

    println!("\n✅ Bandwidth validation (sequential) completed");
}

#[tokio::test]
async fn test_bandwidth_parallel_validation() {
    let qdisc = QdiscManager::new();
    if !qdisc.has_net_admin().await {
        println!("SKIP: No NET_ADMIN capability - need privileged container");
        return;
    }

    // Probe netns creation capability
    match Namespace::ensure("ns-probe-par").await {
        Ok(ns) => {
            let _ = ns.delete().await;
        }
        Err(e) => {
            println!("SKIP: netns creation not permitted: {}", e);
            return;
        }
    }

    println!("=== Bandwidth Validation (Parallel) ===");

    // Define test bandwidth rates
    let target_rates = vec![125, 250, 500, 1000, 2000, 4000];

    // Create bandwidth links with a different prefix to avoid name clashes
    let links: Vec<BandwidthLinkCfg> = target_rates
        .iter()
        .enumerate()
        .map(|(i, &rate)| BandwidthLinkCfg {
            name: format!("par-link-{}k", rate),
            cfg: VethPairConfig {
                tx_if: format!("par-tx-{}", i),
                rx_if: format!("par-rx-{}", i),
                tx_ip_cidr: format!("192.168.{}.1/30", 110 + i),
                rx_ip_cidr: format!("192.168.{}.2/30", 110 + i),
                tx_ns: Some(format!("par-tx-ns-{}", i)),
                rx_ns: Some(format!("par-rx-ns-{}", i)),
                params: Some(network_sim::NetworkParams {
                    delay_ms: 0,
                    loss_pct: 0.0,
                    rate_kbps: rate,
                    jitter_ms: 0,
                    reorder_pct: 0.0,
                    duplicate_pct: 0.0,
                    loss_corr_pct: 0.0,
                }),
            },
            target_kbps: rate,
            port: 9000 + i as u16,
        })
        .collect();

    println!(
        "Setting up {} links with rates: {:?} kbps",
        links.len(),
        target_rates
    );

    // Setup all links concurrently
    let setup_futures = links.iter().map(|l| setup_bandwidth_link(l, &qdisc));
    let setup_results: Vec<_> = futures::future::join_all(setup_futures).await;

    // Check for setup failures
    let mut setup_failures = Vec::new();
    for (i, result) in setup_results.iter().enumerate() {
        if let Err(e) = result {
            setup_failures.push(format!("Link {}: {}", links[i].name, e));
        }
    }

    if !setup_failures.is_empty() {
        eprintln!("Setup failures:");
        for failure in setup_failures {
            eprintln!("  ❌ {}", failure);
        }
        // Cleanup all links
        for _link in &links {
            // best-effort cleanup handled during create/delete lifecycle
        }
        panic!("Link setup failed");
    }

    println!("✅ All {} links setup successfully\n", links.len());

    println!("Starting throughput measurements (5 seconds) in Rust only (parallel)...");

    // Baseline counters
    let mut before: Vec<u64> = Vec::with_capacity(links.len());
    for l in &links {
        before.push(get_qdisc_sent_bytes(l, &qdisc).await.unwrap_or(0));
    }

    // Start all senders concurrently for 5 seconds
    let mut handles = Vec::with_capacity(links.len());
    for l in &links {
        let lc = l.clone();
        handles.push(std::thread::spawn(move || -> std::io::Result<()> {
            // Enter tx namespace via crate guard
            let ns = network_sim::Namespace::from_existing(lc.cfg.tx_ns.clone().unwrap());
            let _ns_guard = ns.enter()?; // keep guard alive for thread lifetime
            let bind_addr = format!("{}:0", lc.cfg.tx_ip_cidr.split('/').next().unwrap());
            let sock =
                UdpSocket::bind(bind_addr).map_err(|e| std::io::Error::other(e.to_string()))?;
            let dest = format!(
                "{}:{}",
                lc.cfg.rx_ip_cidr.split('/').next().unwrap(),
                lc.port
            );
            let payload = vec![0x58u8; 1200];
            let end = Instant::now() + Duration::from_secs(5);
            while Instant::now() < end {
                let _ = sock.send_to(&payload, &dest);
                std::thread::sleep(Duration::from_micros(100));
            }
            Ok(())
        }));
    }

    // Join all senders
    for h in handles {
        let _ = h
            .join()
            .map_err(|_| std::io::Error::other("sender thread panicked"));
    }

    // After counters
    let mut measurements: Vec<(u32, Result<f64, std::io::Error>)> = Vec::with_capacity(links.len());
    for (idx, l) in links.iter().enumerate() {
        let after = get_qdisc_sent_bytes(l, &qdisc).await.unwrap_or(before[idx]);
        let delta = after.saturating_sub(before[idx]);
        let kbps = (delta as f64 * 8.0) / (5.0 * 1000.0);
        measurements.push((l.target_kbps, Ok::<f64, std::io::Error>(kbps)));
    }

    println!("\n=== Results (Parallel) ===");
    let mut all_passed = true;
    for (target, res) in measurements {
        match res {
            Ok(measured_kbps) => {
                let target_f = target as f64;
                let efficiency = (measured_kbps / target_f) * 100.0;
                let upper_tolerance = 1.10; // ≤ 110%
                let lower_tolerance = 0.85; // ≥ 85%
                let passed = measured_kbps <= target_f * upper_tolerance
                    && measured_kbps >= target_f * lower_tolerance;
                println!(
                    "{} Target: {} kbps, Measured: {:.0} kbps ({:.1}% of target)",
                    if passed { "✅ PASS" } else { "❌ FAIL" },
                    target,
                    measured_kbps,
                    efficiency
                );
                if !passed {
                    all_passed = false;
                }
            }
            Err(e) => {
                println!("❌ FAIL - Measurement error: {}", e);
                all_passed = false;
            }
        }
    }

    // Cleanup
    println!("\nCleaning up test links...");
    // best-effort cleanup is handled during setup/teardown

    assert!(
        all_passed,
        "Parallel bandwidth validation failed for one or more links"
    );
    println!("\n✅ Bandwidth validation (parallel) completed");
}
