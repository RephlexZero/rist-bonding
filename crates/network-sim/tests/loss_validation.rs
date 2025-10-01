//! Validate that configured packet loss is approximately observed end-to-end.

use network_sim::qdisc::QdiscManager;
use network_sim::NetworkParams;

#[tokio::test]
async fn test_loss_enforcement_udp() {
    let qdisc = QdiscManager::new();
    if !qdisc.has_net_admin().await {
        eprintln!("Skipping loss test: requires NET_ADMIN");
        return;
    }

    // Build a single veth pair in root namespace
    #[cfg(target_os = "linux")]
    {
        use network_sim::link::{VethPair, VethPairConfig};
        use tokio::net::UdpSocket;
        use tokio::time::{sleep, Duration};

        let tx_if = "veth_loss_tx".to_string();
        let rx_if = "veth_loss_rx".to_string();
        let tx_ip = "10.77.0.1/30".to_string();
        let rx_ip = "10.77.0.2/30".to_string();

        // Configure 10% loss at a moderate rate to keep timing stable
        let target_loss = 0.10f32;
        let rate_kbps = 2000u32;

        let pair = VethPair::create(
            &qdisc,
            &VethPairConfig {
                tx_if: tx_if.clone(),
                rx_if: rx_if.clone(),
                tx_ip_cidr: tx_ip.clone(),
                rx_ip_cidr: rx_ip.clone(),
                tx_ns: None,
                rx_ns: None,
                params: Some(NetworkParams {
                    delay_ms: 0,
                    loss_pct: target_loss,
                    rate_kbps,
                    jitter_ms: 0,
                    reorder_pct: 0.0,
                    duplicate_pct: 0.0,
                    loss_corr_pct: 0.0,
                }),
            },
        )
        .await
        .expect("veth create");

        // Prepare sender/receiver
        let rx_addr = "10.77.0.2:55555";
        let tx_bind = "10.77.0.1:0";

        let recv_sock = UdpSocket::bind(rx_addr).await.expect("bind rx");
        let send_sock = UdpSocket::bind(tx_bind).await.expect("bind tx");

        // Receiver task
        let recv_task = tokio::spawn(async move {
            let mut rcv = 0usize;
            let mut bytes = 0usize;
            let mut buf = vec![0u8; 1500];
            let start = tokio::time::Instant::now();
            while start.elapsed() < Duration::from_secs(7) {
                if let Ok((n, _)) = recv_sock.try_recv_from(&mut buf) {
                    rcv += 1;
                    bytes += n;
                } else {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            }
            (rcv, bytes)
        });

        // Sender loop: paced to ~1.5 Mbps for 4 seconds
        let payload = vec![0xABu8; 200];
        let pps = 900usize; // 900 pkts/s * 200B ≈ 1.44 Mbps
        let total_secs = 4usize;
        let total = pps * total_secs;
        let interval = Duration::from_micros(1_000_000 / pps as u64);
        for _ in 0..total {
            let _ = send_sock.send_to(&payload, rx_addr).await;
            tokio::time::sleep(interval).await;
        }
        // Allow late packets to arrive
        sleep(Duration::from_secs(2)).await;

        let (received, _rx_bytes) = recv_task.await.expect("join recv");
        let sent = total as f64;
        let got = received as f64;
        let loss_measured = ((sent - got) / sent).max(0.0);

        println!(
            "Loss target {:.2}% vs measured {:.2}% (received {} / sent {})",
            target_loss * 100.0,
            loss_measured * 100.0,
            received,
            total
        );

        // Accept some variance due to timing and kernel scheduling
        let tol = 0.05f64; // +/-5 percentage points
        assert!(
            (loss_measured - (target_loss as f64)).abs() <= tol,
            "Measured loss {:.2}% deviates more than {:.2}% from target {:.2}%",
            loss_measured * 100.0,
            tol * 100.0,
            target_loss * 100.0
        );

        // Cleanup
        pair.clear(&qdisc).await.ok();
        pair.delete().await.ok();
    }
}
