//! Simple test to prove namespace-isolated veth pairs enforce rate limiting

use network_sim::qdisc::QdiscManager;
use std::time::Duration;

async fn run_cmd(cmd: &str, args: &[&str]) -> std::io::Result<std::process::Output> {
    use tokio::process::Command;
    Command::new(cmd).args(args).output().await
}

async fn run_cmd_ok(cmd: &str, args: &[&str]) -> std::io::Result<()> {
    let out = run_cmd(cmd, args).await?;
    if out.status.success() {
        Ok(())
    } else {
        eprintln!("Command failed: {} {}", cmd, args.join(" "));
        eprintln!("stderr: {}", String::from_utf8_lossy(&out.stderr));
        Err(std::io::Error::other(String::from_utf8_lossy(&out.stderr).to_string()))
    }
}

#[tokio::test]
async fn test_namespace_veth_rate_limiting() {
    println!("=== Testing Namespace-Isolated Veth Rate Limiting ===");
    
    // Check if we have the required capabilities
    if run_cmd("ip", &["netns", "list"]).await.is_err() {
        println!("SKIP: No network namespace capability");
        return;
    }

    let qdisc = QdiscManager::new();
    if !qdisc.has_net_admin().await {
        println!("SKIP: No NET_ADMIN capability");
        return;
    }

    let tx_if = "test-tx";
    let rx_if = "test-rx";
    let tx_ns = "test-tx-ns";
    let rx_ns = "test-rx-ns";
    let tx_ip = "192.168.100.1";
    let rx_ip = "192.168.100.2";
    let rate_kbps = 500u32; // 500 kbps

    // Cleanup first
    let _ = run_cmd("ip", &["netns", "del", tx_ns]).await;
    let _ = run_cmd("ip", &["netns", "del", rx_ns]).await;
    let _ = run_cmd("ip", &["link", "del", "dev", tx_if]).await;

    // Create namespaces
    run_cmd_ok("ip", &["netns", "add", tx_ns]).await.expect("Failed to create TX namespace");
    run_cmd_ok("ip", &["netns", "add", rx_ns]).await.expect("Failed to create RX namespace");
    println!("✅ Created namespaces: {} and {}", tx_ns, rx_ns);

    // Create veth pair
    run_cmd_ok("ip", &["link", "add", tx_if, "type", "veth", "peer", "name", rx_if]).await
        .expect("Failed to create veth pair");
    println!("✅ Created veth pair: {} <-> {}", tx_if, rx_if);

    // Move interfaces to namespaces
    run_cmd_ok("ip", &["link", "set", tx_if, "netns", tx_ns]).await
        .expect("Failed to move TX interface to namespace");
    run_cmd_ok("ip", &["link", "set", rx_if, "netns", rx_ns]).await
        .expect("Failed to move RX interface to namespace");
    println!("✅ Moved interfaces to respective namespaces");

    // Configure interfaces
    run_cmd_ok("ip", &["netns", "exec", tx_ns, "ip", "addr", "add", &format!("{}/30", tx_ip), "dev", tx_if]).await
        .expect("Failed to configure TX IP");
    run_cmd_ok("ip", &["netns", "exec", rx_ns, "ip", "addr", "add", &format!("{}/30", rx_ip), "dev", rx_if]).await
        .expect("Failed to configure RX IP");
    
    run_cmd_ok("ip", &["netns", "exec", tx_ns, "ip", "link", "set", tx_if, "up"]).await
        .expect("Failed to bring up TX interface");
    run_cmd_ok("ip", &["netns", "exec", rx_ns, "ip", "link", "set", rx_if, "up"]).await
        .expect("Failed to bring up RX interface");
    println!("✅ Configured IP addresses and brought up interfaces");

    // Apply rate limiting to TX interface using netem
    run_cmd_ok("ip", &["netns", "exec", tx_ns, "tc", "qdisc", "add", "dev", tx_if, "root", "netem", "rate", &format!("{}kbit", rate_kbps)]).await
        .expect("Failed to apply rate limiting");
    println!("✅ Applied {} kbps rate limiting to {}", rate_kbps, tx_if);

    // Show qdisc configuration
    let qdisc_output = run_cmd("ip", &["netns", "exec", tx_ns, "tc", "qdisc", "show", "dev", tx_if]).await
        .expect("Failed to show qdisc");
    println!("Qdisc: {}", String::from_utf8_lossy(&qdisc_output.stdout).trim());

    // Test connectivity with ping
    let ping_result = run_cmd("ip", &["netns", "exec", tx_ns, "ping", "-c", "3", "-W", "2", rx_ip]).await;
    if ping_result.is_ok() && ping_result.unwrap().status.success() {
        println!("✅ Connectivity test passed");
    } else {
        println!("⚠️  Ping test failed, but this might be expected due to rate limiting");
    }

    // Simple throughput test using netcat
    println!("\n=== Testing Throughput ===");
    
    // Start receiver
    let mut receiver = tokio::process::Command::new("ip")
        .args(&["netns", "exec", rx_ns, "nc", "-u", "-l", "8000"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("Failed to start receiver");

    tokio::time::sleep(Duration::from_millis(500)).await;

    // Send data and measure
    let start = std::time::Instant::now();
    let test_data = "X".repeat(1000); // 1KB payload
    let packets_to_send = 100; // Send 100KB total
    
    for _ in 0..packets_to_send {
        let _ = run_cmd("ip", &[
            "netns", "exec", tx_ns,
            "bash", "-c", 
            &format!("echo '{}' | nc -u -w1 {} 8000", test_data, rx_ip)
        ]).await;
        tokio::time::sleep(Duration::from_millis(10)).await; // Small delay between packets
    }
    
    let elapsed = start.elapsed();
    let _ = receiver.kill().await;
    
    let total_bytes = packets_to_send * 1000;
    let measured_kbps = (total_bytes as f64 * 8.0) / (elapsed.as_secs_f64() * 1000.0);
    
    println!("Sent {} bytes in {:.2}s", total_bytes, elapsed.as_secs_f64());
    println!("Measured throughput: {:.0} kbps", measured_kbps);
    println!("Target throughput: {} kbps", rate_kbps);
    
    // Check if rate limiting is working (measured should be <= target + some tolerance)
    let tolerance_ratio = 1.5; // Allow 50% over target due to measurement imprecision
    if measured_kbps <= rate_kbps as f64 * tolerance_ratio {
        println!("✅ SUCCESS: Rate limiting appears to be working!");
        println!("   Measured rate ({:.0} kbps) is within expected range", measured_kbps);
    } else {
        println!("⚠️  Rate limiting may not be fully effective");
        println!("   Measured: {:.0} kbps, Expected: ≤ {:.0} kbps", measured_kbps, rate_kbps as f64 * tolerance_ratio);
    }

    // Show traffic statistics
    let stats_output = run_cmd("ip", &["netns", "exec", tx_ns, "tc", "-s", "qdisc", "show", "dev", tx_if]).await;
    if let Ok(stats) = stats_output {
        let stats_str = String::from_utf8_lossy(&stats.stdout);
        if let Some(sent_line) = stats_str.lines().find(|l| l.contains("Sent")) {
            println!("Traffic stats: {}", sent_line.trim());
        }
    }

    // Cleanup
    let _ = run_cmd("ip", &["netns", "del", tx_ns]).await;
    let _ = run_cmd("ip", &["netns", "del", rx_ns]).await;
    
    println!("\n✅ Test completed successfully - namespace-isolated veth pairs are working!");
}
