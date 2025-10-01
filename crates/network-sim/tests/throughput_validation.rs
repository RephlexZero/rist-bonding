//! Validate that network-sim shaping parameters constrain throughput using crate-managed APIs.
//!
//! Sets up veth pairs with distinct rates in separate namespaces, applies egress shaping
//! via QdiscManager, measures UDP throughput with native sockets inside namespaces, and
//! asserts rates match configured values within tolerance.

use network_sim::qdisc::QdiscManager;
use network_sim::{Namespace, VethPair, VethPairConfig};
use std::net::UdpSocket;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
struct TestLinkCfg {
    name: String,
    cfg: VethPairConfig,
    rate_kbps: u32,
    port: u16,
}

async fn setup_link(cfg: &TestLinkCfg, qdisc: &QdiscManager) -> std::io::Result<VethPair> {
    let pair = VethPair::create(qdisc, &cfg.cfg).await?;
    Ok(pair)
}

async fn measure_udp(cfg: &TestLinkCfg, secs: u64) -> std::io::Result<f64> {
    let rx_ns = cfg.cfg.rx_ns.clone().unwrap();
    let tx_ns = cfg.cfg.tx_ns.clone().unwrap();
    let rx_ip = cfg.cfg.rx_ip_cidr.split('/').next().unwrap().to_string();
    let tx_ip = cfg.cfg.tx_ip_cidr.split('/').next().unwrap().to_string();
    let port = cfg.port;

    // Receiver thread
    let recv_handle = std::thread::spawn(move || -> std::io::Result<u64> {
        let ns = Namespace::from_existing(rx_ns);
        let _guard = ns.enter()?;
        let addr = format!("0.0.0.0:{}", port);
        let sock = UdpSocket::bind(addr).map_err(|e| std::io::Error::other(e.to_string()))?;
        sock.set_read_timeout(Some(Duration::from_millis(100))).ok();
        let end = Instant::now() + Duration::from_secs(secs);
        let mut total = 0u64;
        let mut buf = [0u8; 65536];
        while Instant::now() < end {
            match sock.recv(&mut buf) {
                Ok(n) => total += n as u64,
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(e) => return Err(e),
            }
        }
        Ok(total)
    });

    tokio::time::sleep(Duration::from_millis(300)).await;

    // Sender thread
    let send_handle = std::thread::spawn(move || -> std::io::Result<()> {
        let ns = Namespace::from_existing(tx_ns);
        let _guard = ns.enter()?;
        let bind_addr = format!("{}:0", tx_ip);
        let sock = UdpSocket::bind(bind_addr).map_err(|e| std::io::Error::other(e.to_string()))?;
        let dest = format!("{}:{}", rx_ip, port);
        let payload = vec![0x58u8; 1200];
        let end = Instant::now() + Duration::from_secs(secs);
        while Instant::now() < end {
            let _ = sock.send_to(&payload, &dest);
            std::thread::sleep(Duration::from_micros(100));
        }
        Ok(())
    });

    let _ = send_handle
        .join()
        .map_err(|_| std::io::Error::other("send thread panicked"))?;
    let total_bytes = recv_handle
        .join()
        .map_err(|_| std::io::Error::other("recv thread panicked"))?? as f64;
    let kbps = (total_bytes * 8.0) / (secs as f64 * 1000.0);
    Ok(kbps)
}

#[tokio::test]
async fn test_network_shaping_enforcement() {
    let qdisc = QdiscManager::new();
    if !qdisc.has_net_admin().await {
        println!("SKIP: No NET_ADMIN capability - need privileged container");
        return;
    }

    // Probe namespace creation
    match Namespace::ensure("ns-probe-throughput").await {
        Ok(ns) => {
            let _ = ns.delete().await;
        }
        Err(e) => {
            println!("SKIP: netns creation not permitted: {}", e);
            return;
        }
    }

    let links: Vec<TestLinkCfg> = vec![(800u32, 8000u16), (400u32, 8001u16), (150u32, 8002u16)]
        .into_iter()
        .enumerate()
        .map(|(i, (rate, port))| TestLinkCfg {
            name: format!("thr-{}k", rate),
            cfg: VethPairConfig {
                tx_if: format!("thr-tx-{}", i),
                rx_if: format!("thr-rx-{}", i),
                tx_ip_cidr: format!("192.168.{}.1/30", 200 + i),
                rx_ip_cidr: format!("192.168.{}.2/30", 200 + i),
                tx_ns: Some(format!("thr-ns-tx-{}", i)),
                rx_ns: Some(format!("thr-ns-rx-{}", i)),
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
            rate_kbps: rate,
            port,
        })
        .collect();

    // Setup
    let futures = links.iter().map(|l| setup_link(l, &qdisc));
    let results = futures::future::join_all(futures).await;
    for (i, r) in results.iter().enumerate() {
        if let Err(e) = r {
            panic!("Setup failed for {}: {}", links[i].name, e);
        }
    }

    // Measure
    let mut measurements = Vec::new();
    for link in &links {
        let kbps = measure_udp(link, 3).await.expect("measurement failed");
        println!(
            "{}: target={} kbps, measured={:.0} kbps",
            link.name, link.rate_kbps, kbps
        );
        measurements.push((link.rate_kbps as f64, kbps));
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    // Validate within tolerance
    for (i, (target, measured)) in measurements.iter().enumerate() {
        let tol = 0.40; // ±40%
        let lo = target * (1.0 - tol);
        let hi = target * (1.0 + tol);
        assert!(
            *measured >= lo && *measured <= hi,
            "Link {} out of range: target {:.0} kbps, measured {:.0} kbps (allowed [{:.0}, {:.0}])",
            i,
            target,
            measured,
            lo,
            hi
        );
    }

    println!("SUCCESS: All links respect their configured rate limits!");
}
