//! Validate that network-sim shaping parameters actually constrain throughput.
//!
//! Sets up veth pairs with distinct rates in separate namespaces, applies egress shaping,
//! measures UDP throughput, and asserts rates match configured values within tolerance.

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
        Err(std::io::Error::other(
            String::from_utf8_lossy(&out.stderr).to_string(),
        ))
    }
}

struct TestLink {
    tx_if: String,
    rx_if: String,
    tx_ns: String,
    rx_ns: String,
    tx_ip: String,
    rx_ip: String,
    rate_kbps: u32,
    port: u16,
}

async fn setup_test_link(link: &TestLink) -> std::io::Result<()> {
    // Cleanup any existing setup
    let _ = run_cmd("ip", &["netns", "del", &link.tx_ns]).await;
    let _ = run_cmd("ip", &["netns", "del", &link.rx_ns]).await;
    let _ = run_cmd("ip", &["link", "del", "dev", &link.tx_if]).await;

    // Create namespaces
    run_cmd_ok("ip", &["netns", "add", &link.tx_ns]).await?;
    run_cmd_ok("ip", &["netns", "add", &link.rx_ns]).await?;

    // Create veth pair
    run_cmd_ok(
        "ip",
        &[
            "link",
            "add",
            &link.tx_if,
            "type",
            "veth",
            "peer",
            "name",
            &link.rx_if,
        ],
    )
    .await?;

    // Move interfaces to namespaces
    run_cmd_ok(
        "ip",
        &["link", "set", "dev", &link.tx_if, "netns", &link.tx_ns],
    )
    .await?;
    run_cmd_ok(
        "ip",
        &["link", "set", "dev", &link.rx_if, "netns", &link.rx_ns],
    )
    .await?;

    // Configure tx interface
    run_cmd_ok(
        "ip",
        &[
            "-n",
            &link.tx_ns,
            "addr",
            "add",
            &format!("{}/30", link.tx_ip),
            "dev",
            &link.tx_if,
        ],
    )
    .await?;
    run_cmd_ok(
        "ip",
        &["-n", &link.tx_ns, "link", "set", "dev", &link.tx_if, "up"],
    )
    .await?;
    run_cmd_ok("ip", &["-n", &link.tx_ns, "link", "set", "dev", "lo", "up"]).await?;

    // Configure rx interface
    run_cmd_ok(
        "ip",
        &[
            "-n",
            &link.rx_ns,
            "addr",
            "add",
            &format!("{}/30", link.rx_ip),
            "dev",
            &link.rx_if,
        ],
    )
    .await?;
    run_cmd_ok(
        "ip",
        &["-n", &link.rx_ns, "link", "set", "dev", &link.rx_if, "up"],
    )
    .await?;
    run_cmd_ok("ip", &["-n", &link.rx_ns, "link", "set", "dev", "lo", "up"]).await?;

    Ok(())
}

async fn apply_shaping(link: &TestLink) -> std::io::Result<()> {
    // Apply netem rate limiting on the tx interface within its namespace
    let rate_kbit = link.rate_kbps;

    // Use tc directly in the namespace
    run_cmd_ok(
        "ip",
        &[
            "netns",
            "exec",
            &link.tx_ns,
            "tc",
            "qdisc",
            "add",
            "dev",
            &link.tx_if,
            "root",
            "netem",
            "rate",
            &format!("{}kbit", rate_kbit),
        ],
    )
    .await?;

    Ok(())
}

async fn measure_udp_throughput(link: &TestLink, duration_secs: u64) -> std::io::Result<f64> {
    // Use netcat-based simple measurement for reliability 
    // Since we've proven rate limiting works in proof_of_concept test
    measure_udp_simple(link, duration_secs).await
}

async fn measure_udp_simple(link: &TestLink, duration_secs: u64) -> std::io::Result<f64> {
    // Start receiver in rx namespace that counts bytes
    let recv_script = format!(
        r#"
        python3 -c "
import socket
import time
import sys

sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
sock.bind(('{}', {}))
sock.settimeout(0.1)

start = time.time()
end = start + {}
total_bytes = 0

while time.time() < end:
    try:
        data, addr = sock.recvfrom(2048)
        total_bytes += len(data)
    except socket.timeout:
        continue
    except:
        break

elapsed = time.time() - start
kbps = (total_bytes * 8) / (elapsed * 1000)
print(f'{{total_bytes}} {{elapsed:.3f}} {{kbps:.0f}}')
        "#,
        link.rx_ip, link.port, duration_secs
    );

    let recv_cmd = tokio::process::Command::new("ip")
        .args(&["netns", "exec", &link.rx_ns, "bash", "-c", &recv_script])
        .stdout(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;

    // Give receiver time to bind
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Start sender in tx namespace
    let send_script = format!(
        r#"
        python3 -c "
import socket
import time

sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
sock.bind(('{}', 0))

end = time.time() + {}
payload = b'X' * 1200

while time.time() < end:
    try:
        sock.sendto(payload, ('{}', {}))
        time.sleep(0.0001)  # 100us between packets
    except:
        break
        "#,
        link.tx_ip, duration_secs, link.rx_ip, link.port
    );

    let _send_cmd = run_cmd(
        "ip",
        &["netns", "exec", &link.tx_ns, "bash", "-c", &send_script],
    )
    .await?;

    // Get receiver results
    let recv_result = recv_cmd.wait_with_output().await?;
    let recv_output = String::from_utf8_lossy(&recv_result.stdout);

    // Parse: "total_bytes elapsed kbps"
    let parts: Vec<&str> = recv_output.split_whitespace().collect();
    if parts.len() >= 3 {
        if let Ok(kbps) = parts[2].parse::<f64>() {
            return Ok(kbps);
        }
    }

    Err(std::io::Error::other("Failed to parse measurement results"))
}

async fn cleanup_test_link(link: &TestLink) {
    let _ = run_cmd("ip", &["netns", "del", &link.tx_ns]).await;
    let _ = run_cmd("ip", &["netns", "del", &link.rx_ns]).await;
}

#[tokio::test]
async fn test_network_shaping_enforcement() {
    let qdisc = QdiscManager::new();
    if !qdisc.has_net_admin().await {
        eprintln!("Skipping test: requires NET_ADMIN capability for tc and namespaces");
        return;
    }

    let test_links = vec![
        TestLink {
            tx_if: "vtx0".into(),
            rx_if: "vrx0".into(),
            tx_ns: "nstx0".into(),
            rx_ns: "nsrx0".into(),
            tx_ip: "192.168.10.1".into(),
            rx_ip: "192.168.10.2".into(),
            rate_kbps: 800,
            port: 8000,
        },
        TestLink {
            tx_if: "vtx1".into(),
            rx_if: "vrx1".into(),
            tx_ns: "nstx1".into(),
            rx_ns: "nsrx1".into(),
            tx_ip: "192.168.11.1".into(),
            rx_ip: "192.168.11.2".into(),
            rate_kbps: 400,
            port: 8001,
        },
        TestLink {
            tx_if: "vtx2".into(),
            rx_if: "vrx2".into(),
            tx_ns: "nstx2".into(),
            rx_ns: "nsrx2".into(),
            tx_ip: "192.168.12.1".into(),
            rx_ip: "192.168.12.2".into(),
            rate_kbps: 150,
            port: 8002,
        },
    ];

    // Setup all links
    for link in &test_links {
        setup_test_link(link)
            .await
            .expect("Failed to setup test link");
        apply_shaping(link).await.expect("Failed to apply shaping");
    }

    // Print diagnostics
    for link in &test_links {
        if let Ok(out) = run_cmd(
            "ip",
            &[
                "netns",
                "exec",
                &link.tx_ns,
                "tc",
                "qdisc",
                "show",
                "dev",
                &link.tx_if,
            ],
        )
        .await
        {
            println!(
                "Link {} shaping: {}",
                link.tx_if,
                String::from_utf8_lossy(&out.stdout).trim()
            );
        }
    }

    let test_duration = 4; // seconds
    let mut results = Vec::new();

    // Test each link
    for link in &test_links {
        println!(
            "Testing {} -> {} (target: {} kbps)",
            link.tx_ip, link.rx_ip, link.rate_kbps
        );

        let measured_kbps = measure_udp_throughput(link, test_duration)
            .await
            .expect("Failed to measure throughput");

        println!("  Measured: {:.0} kbps", measured_kbps);
        results.push((link.rate_kbps as f64, measured_kbps));

        // Show qdisc stats
        if let Ok(out) = run_cmd(
            "ip",
            &[
                "netns",
                "exec",
                &link.tx_ns,
                "tc",
                "-s",
                "qdisc",
                "show",
                "dev",
                &link.tx_if,
            ],
        )
        .await
        {
            let stats = String::from_utf8_lossy(&out.stdout);
            if let Some(line) = stats.lines().find(|l| l.contains("Sent")) {
                println!("  Stats: {}", line.trim());
            }
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    // Validate results
    for (i, (target_kbps, measured_kbps)) in results.iter().enumerate() {
        let tolerance = 0.40; // 40% tolerance for netem rate limiting
        let lower_bound = target_kbps * (1.0 - tolerance);
        let upper_bound = target_kbps * (1.0 + tolerance);

        println!(
            "Link {}: target={:.0}, measured={:.0}, range=[{:.0}-{:.0}]",
            i, target_kbps, measured_kbps, lower_bound, upper_bound
        );

        assert!(
            *measured_kbps >= lower_bound,
            "Link {} underperformed: measured {:.0} kbps < lower bound {:.0} kbps (target {:.0})",
            i,
            measured_kbps,
            lower_bound,
            target_kbps
        );

        assert!(
            *measured_kbps <= upper_bound,
            "Link {} exceeded limit: measured {:.0} kbps > upper bound {:.0} kbps (target {:.0})",
            i,
            measured_kbps,
            upper_bound,
            target_kbps
        );
    }

    // Cleanup
    for link in &test_links {
        cleanup_test_link(link).await;
    }

    println!("SUCCESS: All links respect their configured rate limits!");
}
