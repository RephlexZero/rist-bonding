//! Functional test: create 4 independent links with different bitrates, apply egress and ingress
//! shaping, and verify UDP packets can flow across each link.

use network_sim::{qdisc::QdiscManager, runtime::{apply_ingress_params, apply_network_params, remove_ingress_params, remove_network_params}, types::NetworkParams};
use std::time::Duration;
use tokio::{net::UdpSocket, time::timeout};

async fn run_cmd(cmd: &str, args: &[&str]) -> std::io::Result<std::process::Output> {
    use tokio::process::Command;
    Command::new(cmd).args(args).output().await
}

async fn setup_veth_pair(tx: &str, rx: &str, tx_ip: &str, rx_ip: &str) -> std::io::Result<()> {
    // best-effort cleanup
    let _ = run_cmd("ip", &["link", "del", "dev", tx]).await;
    let _ = run_cmd("ip", &["link", "del", "dev", rx]).await;

    // create
    let out = run_cmd("ip", &["link", "add", tx, "type", "veth", "peer", "name", rx]).await?;
    if !out.status.success() { return Err(std::io::Error::other(String::from_utf8_lossy(&out.stderr).to_string())); }

    // assign /30 and bring up
    let _ = run_cmd("ip", &["addr", "flush", "dev", tx]).await;
    let _ = run_cmd("ip", &["addr", "flush", "dev", rx]).await;
    let out = run_cmd("ip", &["addr", "add", &format!("{}/30", tx_ip), "dev", tx]).await?;
    if !out.status.success() { return Err(std::io::Error::other(String::from_utf8_lossy(&out.stderr).to_string())); }
    let out = run_cmd("ip", &["addr", "add", &format!("{}/30", rx_ip), "dev", rx]).await?;
    if !out.status.success() { return Err(std::io::Error::other(String::from_utf8_lossy(&out.stderr).to_string())); }
    let out = run_cmd("ip", &["link", "set", "dev", tx, "up"]).await?;
    if !out.status.success() { return Err(std::io::Error::other(String::from_utf8_lossy(&out.stderr).to_string())); }
    let out = run_cmd("ip", &["link", "set", "dev", rx, "up"]).await?;
    if !out.status.success() { return Err(std::io::Error::other(String::from_utf8_lossy(&out.stderr).to_string())); }
    Ok(())
}

async fn cleanup_veth_pair(tx: &str, rx: &str) {
    let _ = run_cmd("ip", &["link", "del", "dev", tx]).await;
    let _ = run_cmd("ip", &["link", "del", "dev", rx]).await;
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

    // Setup and shape
    for l in &links {
        setup_veth_pair(&l.tx, &l.rx, &l.tx_ip, &l.rx_ip).await.expect("veth setup");
        let params = NetworkParams { delay_ms: 10 + (l.port as u32 % 20), loss_pct: 0.0, rate_kbps: l.rate, jitter_ms: 0, reorder_pct: 0.0, duplicate_pct: 0.0, loss_corr_pct: 0.0 };
        let _ = apply_network_params(&qdisc, &l.tx, &params).await;       // egress from sender
        let _ = apply_ingress_params(&qdisc, &l.rx, &params).await;       // ingress to receiver
    }

    // For each link, bring up a UDP receiver and send a packet
    for l in &links {
        // Receiver on rx_ip:port
        let recv_addr = format!("{}:{}", l.rx_ip, l.port);
        let sender_bind = format!("{}:0", l.tx_ip);

        let socket_recv = UdpSocket::bind(&recv_addr).await.expect("bind rx");

        // Spawn receiver task waiting for one datagram
        let recv_task = tokio::spawn(async move {
            let mut buf = [0u8; 1500];
            let res = timeout(Duration::from_secs(3), socket_recv.recv_from(&mut buf)).await;
            res.map(|r| r.map(|(_n, _peer)| ())).map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout"))
        });

        // Sender binds to tx_ip and sends to rx_ip:port
        let socket_tx = UdpSocket::bind(&sender_bind).await.expect("bind tx");
        let _sent = socket_tx.send_to(b"hello", &recv_addr).await.expect("send");

        // Await receiver
        match recv_task.await.expect("join receiver") {
            Ok(Ok(())) => { /* ok */ }
            Ok(Err(e)) => panic!("receive failed on {}->{}: {}", l.tx, l.rx, e),
            Err(join_err) => panic!("receiver task join error on {}->{}: {}", l.tx, l.rx, join_err),
        }
    }

    // Cleanup shaping and links
    for l in &links {
        let _ = remove_network_params(&qdisc, &l.tx).await;
        let _ = remove_ingress_params(&qdisc, &l.rx).await;
        cleanup_veth_pair(&l.tx, &l.rx).await;
    }
}
