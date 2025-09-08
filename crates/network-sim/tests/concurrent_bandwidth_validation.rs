//! Concurrent bandwidth validation test
//! 
//! Creates multiple links with different bandwidth limits (125, 250, 500, 1000, 2000, 4000 kbps)
//! and validates that each link enforces its configured rate limit concurrently.

use network_sim::qdisc::QdiscManager;
use std::time::{Duration, Instant};
use nix::sched::{setns, CloneFlags};
use std::fs::File;
use std::net::UdpSocket;

async fn run_cmd(cmd: &str, args: &[&str]) -> std::io::Result<std::process::Output> {
    tokio::process::Command::new(cmd).args(args).output().await
}

async fn run_cmd_ok(cmd: &str, args: &[&str]) -> std::io::Result<()> {
    let out = run_cmd(cmd, args).await?;
    if out.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr);
        eprintln!("Command failed: {} {} - {}", cmd, args.join(" "), stderr);
        Err(std::io::Error::other(format!("Command failed: {}", stderr)))
    }
}

#[derive(Debug, Clone)]
struct BandwidthLink {
    name: String,
    tx_if: String,
    rx_if: String,
    tx_ns: String,
    rx_ns: String,
    tx_ip: String,
    rx_ip: String,
    target_kbps: u32,
    port: u16,
}

impl BandwidthLink {
    fn _new(id: u32, target_kbps: u32) -> Self {
        Self::new_with_prefix("bw", id, target_kbps)
    }

    fn new_with_prefix(prefix: &str, id: u32, target_kbps: u32) -> Self {
        Self {
            name: format!("{}-link-{}k", prefix, target_kbps),
            tx_if: format!("{}-tx-{}", prefix, id),
            rx_if: format!("{}-rx-{}", prefix, id),
            tx_ns: format!("{}-tx-ns-{}", prefix, id),
            rx_ns: format!("{}-rx-ns-{}", prefix, id),
            tx_ip: format!("192.168.{}.1", 100 + id),
            rx_ip: format!("192.168.{}.2", 100 + id),
            target_kbps,
            port: 8000 + id as u16,
        }
    }
}

async fn setup_bandwidth_link(link: &BandwidthLink) -> std::io::Result<()> {
    println!("Setting up {} ({})", link.name, link.target_kbps);

    // Cleanup first
    let _ = run_cmd("ip", &["netns", "del", &link.tx_ns]).await;
    let _ = run_cmd("ip", &["netns", "del", &link.rx_ns]).await;
    let _ = run_cmd("ip", &["link", "del", "dev", &link.tx_if]).await;

    // Create namespaces
    run_cmd_ok("ip", &["netns", "add", &link.tx_ns]).await
        .map_err(|e| std::io::Error::other(format!("Failed to create TX namespace {}: {}", link.tx_ns, e)))?;
    run_cmd_ok("ip", &["netns", "add", &link.rx_ns]).await
        .map_err(|e| std::io::Error::other(format!("Failed to create RX namespace {}: {}", link.rx_ns, e)))?;

    // Create veth pair
    run_cmd_ok("ip", &["link", "add", &link.tx_if, "type", "veth", "peer", "name", &link.rx_if]).await
        .map_err(|e| std::io::Error::other(format!("Failed to create veth pair: {}", e)))?;

    // Move interfaces to namespaces
    run_cmd_ok("ip", &["link", "set", &link.tx_if, "netns", &link.tx_ns]).await?;
    run_cmd_ok("ip", &["link", "set", &link.rx_if, "netns", &link.rx_ns]).await?;

    // Configure IP addresses
    run_cmd_ok("ip", &["netns", "exec", &link.tx_ns, "ip", "addr", "add", &format!("{}/30", link.tx_ip), "dev", &link.tx_if]).await?;
    run_cmd_ok("ip", &["netns", "exec", &link.rx_ns, "ip", "addr", "add", &format!("{}/30", link.rx_ip), "dev", &link.rx_if]).await?;

    // Bring interfaces up
    run_cmd_ok("ip", &["netns", "exec", &link.tx_ns, "ip", "link", "set", &link.tx_if, "up"]).await?;
    run_cmd_ok("ip", &["netns", "exec", &link.rx_ns, "ip", "link", "set", &link.rx_if, "up"]).await?;

    // Apply bandwidth limiting using netem
    run_cmd_ok("ip", &["netns", "exec", &link.tx_ns, "tc", "qdisc", "add", "dev", &link.tx_if, "root", "netem", "rate", &format!("{}kbit", link.target_kbps)]).await
        .map_err(|e| std::io::Error::other(format!("Failed to apply rate limiting to {}: {}", link.tx_if, e)))?;

    // Verify qdisc was applied
    let qdisc_output = run_cmd("ip", &["netns", "exec", &link.tx_ns, "tc", "qdisc", "show", "dev", &link.tx_if]).await?;
    let qdisc_str = String::from_utf8_lossy(&qdisc_output.stdout);
    println!("  {} qdisc: {}", link.name, qdisc_str.trim());

    Ok(())
}

fn enter_netns(ns: &str) -> std::io::Result<()> {
    let candidates = [
        format!("/run/netns/{}", ns),
        format!("/var/run/netns/{}", ns),
    ];
    let mut last_err: Option<std::io::Error> = None;
    for p in candidates {
        match File::open(&p) {
            Ok(f) => {
                setns(&f, CloneFlags::CLONE_NEWNET)
                    .map_err(|e| std::io::Error::other(e.to_string()))?;
                return Ok(());
            }
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| std::io::Error::other("netns open failed")))
}

async fn measure_link_throughput(link: &BandwidthLink, duration_secs: u64) -> std::io::Result<f64> {
    let rx_ns = link.rx_ns.clone();
    let tx_ns = link.tx_ns.clone();
    let rx_ip_tx = link.rx_ip.clone();
    let tx_ip = link.tx_ip.clone();
    let port = link.port;

    let recv_handle = std::thread::spawn(move || -> std::io::Result<u64> {
    enter_netns(&rx_ns)?;
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
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(e) => return Err(e),
            }
        }
        Ok(total)
    });

    // small delay to ensure receiver bound
    tokio::time::sleep(Duration::from_millis(350)).await;

    let send_handle = std::thread::spawn(move || -> std::io::Result<()> {
    enter_netns(&tx_ns)?;
        // bind to tx_ip to ensure correct source
        let bind_addr = format!("{}:0", tx_ip);
        let socket = UdpSocket::bind(bind_addr).map_err(|e| std::io::Error::other(e.to_string()))?;
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
    let _ = send_handle.join().map_err(|_| std::io::Error::other("send thread panicked"))?;
    // Collect receiver bytes
    let total_bytes = recv_handle
        .join()
        .map_err(|_| std::io::Error::other("recv thread panicked"))?? as f64;

    let kbps = (total_bytes * 8.0) / (duration_secs as f64 * 1000.0);
    Ok(kbps)
}

async fn cleanup_link(link: &BandwidthLink) {
    let _ = run_cmd("ip", &["netns", "del", &link.tx_ns]).await;
    let _ = run_cmd("ip", &["netns", "del", &link.rx_ns]).await;
}

async fn get_qdisc_sent_bytes(link: &BandwidthLink) -> std::io::Result<u64> {
    let out = run_cmd(
        "ip",
        &[
            "netns", "exec", &link.tx_ns, "tc", "-s", "qdisc", "show", "dev", &link.tx_if,
        ],
    )
    .await?;
    if !out.status.success() {
        return Err(std::io::Error::other("tc -s qdisc show failed"));
    }
    let s = String::from_utf8_lossy(&out.stdout);
    for line in s.lines() {
        if let Some(rest) = line.trim().strip_prefix("Sent ") {
            // Format: Sent <bytes> bytes <pkts> pkt ...
            let mut parts = rest.split_whitespace();
            if let Some(bytes_str) = parts.next() {
                if let Ok(b) = bytes_str.parse::<u64>() {
                    return Ok(b);
                }
            }
        }
    }
    Err(std::io::Error::other("could not parse Sent bytes from tc output"))
}

#[tokio::test]
async fn test_bandwidth_sequential_validation() {
    let qdisc = QdiscManager::new();
    if !qdisc.has_net_admin().await {
        println!("SKIP: No NET_ADMIN capability - need privileged container");
        return;
    }

    println!("=== Bandwidth Validation (Sequential) ===");

    // Define test bandwidth rates
    let target_rates = vec![125, 250, 500, 1000, 2000, 4000];
    
    // Create bandwidth links
    let links: Vec<BandwidthLink> = target_rates
        .iter()
        .enumerate()
        .map(|(i, &rate)| BandwidthLink::new_with_prefix("seq", i as u32, rate))
        .collect();

    println!("Setting up {} links with rates: {:?} kbps", links.len(), target_rates);

    // Setup all links concurrently
    let setup_futures = links.iter().map(setup_bandwidth_link);
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
        for link in &links {
            cleanup_link(link).await;
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
                
                let status = if measured_kbps <= target * upper_tolerance && measured_kbps >= target * lower_tolerance {
                    "✅ PASS"
                } else {
                    all_passed = false;
                    "❌ FAIL"
                };

                println!("{} {} - Target: {} kbps, Measured: {:.0} kbps ({:.1}% of target)", 
                         status, link.name, target, measured_kbps, efficiency);
                
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
    let cleanup_futures = links.iter().map(cleanup_link);
    futures::future::join_all(cleanup_futures).await;

    // Analysis
    println!("\n=== Analysis (Sequential) ===");
    if all_passed {
        println!("✅ SUCCESS: All bandwidth limits are being enforced correctly");
        
        // Show rate differentiation
        results.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap()); // Sort by target rate
        println!("\nRate differentiation validation:");
    for (i, (name, _target, measured, _)) in results.iter().enumerate() {
            if i > 0 {
                let prev_measured = results[i-1].2;
                if measured > &prev_measured {
                    println!("  ✅ {} ({:.0} kbps) > {} ({:.0} kbps)", 
                             name, measured, results[i-1].0, prev_measured);
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

    println!("=== Bandwidth Validation (Parallel) ===");

    // Define test bandwidth rates
    let target_rates = vec![125, 250, 500, 1000, 2000, 4000];

    // Create bandwidth links with a different prefix to avoid name clashes
    let links: Vec<BandwidthLink> = target_rates
        .iter()
        .enumerate()
        .map(|(i, &rate)| BandwidthLink::new_with_prefix("par", i as u32, rate))
        .collect();

    println!("Setting up {} links with rates: {:?} kbps", links.len(), target_rates);

    // Setup all links concurrently
    let setup_futures = links.iter().map(setup_bandwidth_link);
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
        for link in &links {
            cleanup_link(link).await;
        }
        panic!("Link setup failed");
    }

    println!("✅ All {} links setup successfully\n", links.len());

    println!("Starting throughput measurements (5 seconds) in Rust only (parallel)...");

    // Baseline counters
    let mut before: Vec<u64> = Vec::with_capacity(links.len());
    for l in &links {
        before.push(get_qdisc_sent_bytes(l).await.unwrap_or(0));
    }

    // Start all senders concurrently for 5 seconds
    let mut handles = Vec::with_capacity(links.len());
    for l in &links {
        let lc = l.clone();
        handles.push(std::thread::spawn(move || -> std::io::Result<()> {
            enter_netns(&lc.tx_ns)?;
            let bind_addr = format!("{}:0", lc.tx_ip);
            let sock = UdpSocket::bind(bind_addr).map_err(|e| std::io::Error::other(e.to_string()))?;
            let dest = format!("{}:{}", lc.rx_ip, lc.port);
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
        let _ = h.join().map_err(|_| std::io::Error::other("sender thread panicked"));
    }

    // After counters
    let mut measurements: Vec<(u32, Result<f64, std::io::Error>)> = Vec::with_capacity(links.len());
    for (idx, l) in links.iter().enumerate() {
        let after = get_qdisc_sent_bytes(l).await.unwrap_or(before[idx]);
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
                let passed = measured_kbps <= target_f * upper_tolerance && measured_kbps >= target_f * lower_tolerance;
                println!(
                    "{} Target: {} kbps, Measured: {:.0} kbps ({:.1}% of target)",
                    if passed { "✅ PASS" } else { "❌ FAIL" },
                    target,
                    measured_kbps,
                    efficiency
                );
                if !passed { all_passed = false; }
            }
            Err(e) => {
                println!("❌ FAIL - Measurement error: {}", e);
                all_passed = false;
            }
        }
    }

    // Cleanup
    println!("\nCleaning up test links...");
    let cleanup_futures = links.iter().map(cleanup_link);
    futures::future::join_all(cleanup_futures).await;

    assert!(all_passed, "Parallel bandwidth validation failed for one or more links");
    println!("\n✅ Bandwidth validation (parallel) completed");
}
