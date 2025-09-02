//! Functional test: create 4 independent links with different bitrates, apply egress and ingress
//! shaping, and verify UDP packets can flow across each link.

use network_sim::{
    qdisc::QdiscManager,
    runtime::{apply_ingress_params, apply_network_params, remove_ingress_params, remove_network_params},
    types::NetworkParams,
};
use std::time::Duration;
use tokio::{net::UdpSocket, time::timeout};
use futures::future::try_join_all;

/// Helper: run a command and return Output
async fn run_cmd(cmd: &str, args: &[&str]) -> std::io::Result<std::process::Output> {
    use tokio::process::Command;
    Command::new(cmd).args(args).output().await
}

/// Centralized command execution with success check
async fn run_cmd_ok(cmd: &str, args: &[&str]) -> std::io::Result<()> {
    let out = run_cmd(cmd, args).await?;
    if out.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        Err(std::io::Error::other(stderr))
    }
}

async fn setup_veth_pair(tx: &str, rx: &str, tx_ip: &str, rx_ip: &str) -> std::io::Result<()> {
    // best-effort cleanup
    let _ = run_cmd("ip", &["link", "del", "dev", tx]).await;
    let _ = run_cmd("ip", &["link", "del", "dev", rx]).await;

    // create
    run_cmd_ok("ip", &["link", "add", tx, "type", "veth", "peer", "name", rx]).await?;

    // assign /30 and bring up
    let _ = run_cmd("ip", &["addr", "flush", "dev", tx]).await;
    let _ = run_cmd("ip", &["addr", "flush", "dev", rx]).await;
    run_cmd_ok("ip", &["addr", "add", &format!("{}/30", tx_ip), "dev", tx]).await?;
    run_cmd_ok("ip", &["addr", "add", &format!("{}/30", rx_ip), "dev", rx]).await?;
    run_cmd_ok("ip", &["link", "set", "dev", tx, "up"]).await?;
    run_cmd_ok("ip", &["link", "set", "dev", rx, "up"]).await?;
    Ok(())
}

async fn cleanup_veth_pair(tx: &str, rx: &str) {
    let _ = run_cmd("ip", &["link", "del", "dev", tx]).await;
    let _ = run_cmd("ip", &["link", "del", "dev", rx]).await;
}

/// RAII guard to ensure shaping and veth are cleaned up even if the test panics
struct LinkGuard {
    qdisc: QdiscManager,
    tx: String,
    rx: String,
    shaped: bool,
}

impl LinkGuard {
    async fn new(qdisc: &QdiscManager, tx: &str, rx: &str, tx_ip: &str, rx_ip: &str, params: &NetworkParams) -> std::io::Result<Self> {
        setup_veth_pair(tx, rx, tx_ip, rx_ip).await?;
        // Assert shaping succeeds
        apply_network_params(qdisc, tx, params).await.expect("apply egress params");
        apply_ingress_params(qdisc, rx, params).await.expect("apply ingress params");
        Ok(Self { qdisc: QdiscManager::new(), tx: tx.to_string(), rx: rx.to_string(), shaped: true })
    }

    /// Explicit async cleanup to run before guard drops
    async fn cleanup(&mut self) {
        if self.shaped {
            let _ = remove_network_params(&self.qdisc, &self.tx).await;
            let _ = remove_ingress_params(&self.qdisc, &self.rx).await;
            self.shaped = false;
        }
        cleanup_veth_pair(&self.tx, &self.rx).await;
    }
}

impl Drop for LinkGuard {
    fn drop(&mut self) {
        // Best-effort synchronous cleanup; cannot await here
        // 1) tc qdisc del dev <tx> root
        let _ = std::process::Command::new("tc").args(["qdisc", "del", "dev", &self.tx, "root"]).output();
        // 2) tc qdisc del dev <rx> ingress
        let _ = std::process::Command::new("tc").args(["qdisc", "del", "dev", &self.rx, "ingress"]).output();
        // 3) ip link del dev <tx> (deletes veth pair)
        let _ = std::process::Command::new("ip").args(["link", "del", "dev", &self.tx]).output();
    }
}

#[tokio::test]
async fn test_four_links_udp_flow_with_ingress() {
    let qdisc = QdiscManager::new();
    if !qdisc.has_net_admin().await {
        eprintln!("skipping: requires NET_ADMIN for tc and veth");
        return;
    }

    // Define 4 links with distinct bitrates; keep loss 0 to avoid flakiness
    struct Link { tx: String, rx: String, tx_ip: String, rx_ip: String, rate: u32, port: u16 }
    let links: Vec<Link> = (0..4).map(|i| Link {
        tx: format!("veths{}", i),
        rx: format!("vethr{}", i),
        tx_ip: format!("10.210.{}.1", 100 + i),
        rx_ip: format!("10.210.{}.2", 100 + i),
        rate: match i { 0 => 4000, 1 => 2000, 2 => 1200, _ => 800 },
        port: 6000 + i as u16,
    }).collect();

    // Setup and shape with guards (assert success)
    let mut guards = Vec::with_capacity(links.len());
    for l in &links {
        let params = NetworkParams {
            delay_ms: 10 + (l.port as u32 % 20),
            loss_pct: 0.0,
            rate_kbps: l.rate,
            jitter_ms: 0,
            reorder_pct: 0.0,
            duplicate_pct: 0.0,
            loss_corr_pct: 0.0,
        };
        let guard = LinkGuard::new(&qdisc, &l.tx, &l.rx, &l.tx_ip, &l.rx_ip, &params)
            .await
            .expect("veth + shaping setup");
        guards.push(guard);
    }

    // Bring up receivers and send packets on all links concurrently (aggregated with try_join_all)
    let mut tasks = Vec::with_capacity(links.len());
    for l in &links {
        let recv_addr = format!("{}:{}", l.rx_ip, l.port);
        let sender_bind = format!("{}:0", l.tx_ip);
        let tx_name = l.tx.clone();
        let rx_name = l.rx.clone();
        let fut = async move {
            let socket_recv = UdpSocket::bind(&recv_addr)
                .await
                .map_err(|e| format!("bind rx {}: {}", rx_name, e))?;
            // Spawn receiver for one datagram
            let recv_once = tokio::spawn(async move {
                let mut buf = [0u8; 1500];
                let res = timeout(Duration::from_secs(3), socket_recv.recv_from(&mut buf)).await;
                res.map(|r| r.map(|(_n, _peer)| ())).map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout"))
            });

            // Sender side
            let socket_tx = UdpSocket::bind(&sender_bind)
                .await
                .map_err(|e| format!("bind tx {}: {}", tx_name, e))?;
            socket_tx
                .send_to(b"hello", &recv_addr)
                .await
                .map_err(|e| format!("send {}->{}: {}", tx_name, rx_name, e))?;

            match recv_once
                .await
                .map_err(|e| format!("receiver join error on {}->{}: {}", tx_name, rx_name, e))? {
                Ok(Ok(())) => Ok::<(), String>(()),
                Ok(Err(e)) => Err(format!("receive failed on {}->{}: {}", tx_name, rx_name, e)),
                Err(e) => Err(format!("receive timeout/error on {}->{}: {}", tx_name, rx_name, e)),
            }
        };
        tasks.push(fut);
    }

    try_join_all(tasks).await.expect("concurrent UDP flow across all links");

    // Explicit cleanup via guards to ensure awaited teardown; Drop handles best-effort on panic
    for g in &mut guards {
        g.cleanup().await;
    }
}
