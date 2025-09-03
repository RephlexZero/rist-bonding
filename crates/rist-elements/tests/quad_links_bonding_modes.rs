//! Quad-link RIST bonding tests using built-in broadcast and round-robin
//! modes in ristsink. Uses network-sim to create four veth links.

use gst::prelude::*;
use gstreamer as gst;
use gstristelements::testing::init_for_tests;
use std::time::Duration;

#[cfg(feature = "network-sim")]
use ::network_sim::{
    qdisc::QdiscManager,
    runtime::{apply_ingress_params, apply_network_params, remove_ingress_params, remove_network_params},
    types::NetworkParams,
};

#[cfg(feature = "network-sim")]
use std::sync::Arc;

#[cfg(feature = "network-sim")]
use tokio::time::sleep;


#[cfg(feature = "network-sim")]
async fn has_net_admin() -> bool {
    QdiscManager::default().has_net_admin().await
}

#[cfg(feature = "network-sim")]
async fn run_cmd(cmd: &str, args: &[&str]) -> std::io::Result<std::process::Output> {
    use tokio::process::Command;
    Command::new(cmd).args(args).output().await
}

#[cfg(feature = "network-sim")]
async fn setup_veth_pair(tx: &str, rx: &str, tx_ip: &str, rx_ip: &str) -> std::io::Result<()> {
    let _ = run_cmd("ip", &["link", "del", "dev", tx]).await;
    let _ = run_cmd("ip", &["link", "del", "dev", rx]).await;

    let out = run_cmd("ip", &["link", "add", tx, "type", "veth", "peer", "name", rx]).await?;
    if !out.status.success() {
        return Err(std::io::Error::other(String::from_utf8_lossy(&out.stderr).to_string()));
    }
    let _ = run_cmd("ip", &["addr", "flush", "dev", tx]).await;
    let _ = run_cmd("ip", &["addr", "flush", "dev", rx]).await;
    let out = run_cmd("ip", &["addr", "add", &format!("{}/30", tx_ip), "dev", tx]).await?;
    if !out.status.success() {
        return Err(std::io::Error::other(String::from_utf8_lossy(&out.stderr).to_string()));
    }
    let out = run_cmd("ip", &["addr", "add", &format!("{}/30", rx_ip), "dev", rx]).await?;
    if !out.status.success() {
        return Err(std::io::Error::other(String::from_utf8_lossy(&out.stderr).to_string()));
    }
    let out = run_cmd("ip", &["link", "set", "dev", tx, "up"]).await?;
    if !out.status.success() {
        return Err(std::io::Error::other(String::from_utf8_lossy(&out.stderr).to_string()));
    }
    let out = run_cmd("ip", &["link", "set", "dev", rx, "up"]).await?;
    if !out.status.success() {
        return Err(std::io::Error::other(String::from_utf8_lossy(&out.stderr).to_string()));
    }
    Ok(())
}

#[cfg(feature = "network-sim")]
async fn cleanup_veth_pair(tx: &str, rx: &str) {
    let _ = run_cmd("ip", &["link", "del", "dev", tx]).await;
    let _ = run_cmd("ip", &["link", "del", "dev", rx]).await;
}

#[cfg(feature = "network-sim")]
struct VethGuard { qdisc: Arc<QdiscManager>, veth_tx: String, veth_rx: String }

#[cfg(feature = "network-sim")]
impl VethGuard {
    async fn setup(veth_tx: &str, veth_rx: &str, tx_ip: &str, rx_ip: &str, params: &NetworkParams, qdisc: Arc<QdiscManager>) -> anyhow::Result<Self> {
        setup_veth_pair(veth_tx, veth_rx, tx_ip, rx_ip).await
            .map_err(|e| anyhow::anyhow!("veth setup {}-{} failed: {}", veth_tx, veth_rx, e))?;
        apply_network_params(&qdisc, veth_tx, params).await
            .map_err(|e| anyhow::anyhow!("egress qdisc apply failed on {}: {}", veth_tx, e))?;
        apply_ingress_params(&qdisc, veth_rx, params).await
            .map_err(|e| anyhow::anyhow!("ingress qdisc apply failed on {}: {}", veth_rx, e))?;
        Ok(Self { qdisc, veth_tx: veth_tx.to_string(), veth_rx: veth_rx.to_string() })
    }
}

#[cfg(feature = "network-sim")]
impl Drop for VethGuard {
    fn drop(&mut self) {
        let q = self.qdisc.clone();
        let tx = self.veth_tx.clone();
        let rx = self.veth_rx.clone();
        tokio::spawn(async move {
            let _ = remove_network_params(&q, &tx).await;
            let _ = remove_ingress_params(&q, &rx).await;
            cleanup_veth_pair(&tx, &rx).await;
        });
    }
}

#[cfg(feature = "network-sim")]
#[tokio::test]
async fn quad_links_broadcast_and_roundrobin() {
    init_for_tests();

    if gst::ElementFactory::make("ristsink").build().is_err() || gst::ElementFactory::make("ristsrc").build().is_err() {
        println!("Skipping: RIST elements not available");
        return;
    }
    if !has_net_admin().await { println!("Skipping: requires NET_ADMIN"); return; }

    println!("=== Quad-link RIST bonding: broadcast then round-robin ===");

    let qdisc = Arc::new(QdiscManager::new());
    // 4 links with similar params
    let params = NetworkParams { delay_ms: 20, jitter_ms: 0, loss_pct: 0.001, loss_corr_pct: 0.0, duplicate_pct: 0.0, reorder_pct: 0.0, rate_kbps: 9000 };

    // Allocate names/ips/ports
    let base_port: u32 = 6600;
    let links = vec![
        ("vethsB1", "vethrB1", "10.210.0.1", "10.210.0.2", base_port),
        ("vethsB2", "vethrB2", "10.210.0.5", "10.210.0.6", base_port + 2),
        ("vethsB3", "vethrB3", "10.210.0.9", "10.210.0.10", base_port + 4),
        ("vethsB4", "vethrB4", "10.210.0.13", "10.210.0.14", base_port + 6),
    ];

    // Setup veths
    let mut guards = Vec::new();
    for (tx, rx, tx_ip, rx_ip, _port) in &links {
        let g = VethGuard::setup(tx, rx, tx_ip, rx_ip, &params, qdisc.clone()).await.expect("veth setup failed");
        println!("Link: {}({}) -> {}({})", tx, tx_ip, rx, rx_ip);
        guards.push(g);
    }

    // Helper to build sender/receiver with chosen bonding-method
    let build_pipelines = |bonding_method: &str| -> (gst::Pipeline, gst::Pipeline, gst::Element) {
        // Sender
        let sender = gst::Pipeline::new();
        let videotestsrc = gst::ElementFactory::make("videotestsrc").property("is-live", true).property_from_str("pattern", "smpte").build().unwrap();
        let videoconvert = gst::ElementFactory::make("videoconvert").build().unwrap();
        let capsfilter = gst::ElementFactory::make("capsfilter")
            .property("caps", gst::Caps::builder("video/x-raw").field("format", "I420").field("width", 1920i32).field("height", 1080i32).field("framerate", gst::Fraction::new(60, 1)).build())
            .build().unwrap();
        let x265enc = gst::ElementFactory::make("x265enc").property("bitrate", 3000u32).property_from_str("speed-preset", "ultrafast").property_from_str("tune", "zerolatency").build().unwrap();
        let h265parse = gst::ElementFactory::make("h265parse").build().unwrap();
        let rtph265pay = gst::ElementFactory::make("rtph265pay").build().unwrap();

    // Sender must target the receiver IPs (4th tuple element)
    let bonding_addresses = links.iter().map(|(_,_, _tx_ip, rx_ip, port)| format!("{}:{}", rx_ip, port)).collect::<Vec<_>>().join(",");
        let ristsink = gst::ElementFactory::make("ristsink")
            .property("bonding-addresses", &bonding_addresses)
            .property_from_str("bonding-method", bonding_method)
            .property("sender-buffer", 1000u32)
            .property("stats-update-interval", 500u32)
            .build().unwrap();

        sender.add_many([&videotestsrc, &videoconvert, &capsfilter, &x265enc, &h265parse, &rtph265pay, &ristsink]).unwrap();
        gst::Element::link_many([&videotestsrc, &videoconvert, &capsfilter, &x265enc, &h265parse, &rtph265pay, &ristsink]).unwrap();

        // Receiver
        let receiver = gst::Pipeline::new();
    // Receiver expects remote sender addresses (3rd tuple element)
    let bonding_addresses_rx = links.iter().map(|(_,_, tx_ip, _rx_ip, port)| format!("{}:{}", tx_ip, port)).collect::<Vec<_>>().join(",");
        let ristsrc = gst::ElementFactory::make("ristsrc")
            .property("address", "0.0.0.0")
            .property("port", links[0].4)
            .property("bonding-addresses", &bonding_addresses_rx)
            .property("encoding-name", "H265")
            .property("caps", gst::Caps::builder("application/x-rtp").field("media", "video").field("encoding-name", "H265").build())
            .property("receiver-buffer", 2000u32)
            .build().unwrap();
        let rtph265depay = gst::ElementFactory::make("rtph265depay").build().unwrap();
        let h265parse_rx = gst::ElementFactory::make("h265parse").build().unwrap();
        let avdec_h265 = gst::ElementFactory::make("avdec_h265").build().unwrap();
        let videoconvert_rx = gst::ElementFactory::make("videoconvert").build().unwrap();
        let appsink = gst::ElementFactory::make("appsink").property("sync", false).property("drop", true).build().unwrap();
        receiver.add_many([&ristsrc, &rtph265depay, &h265parse_rx, &avdec_h265, &videoconvert_rx, &appsink]).unwrap();
        gst::Element::link_many([&ristsrc, &rtph265depay, &h265parse_rx, &avdec_h265, &videoconvert_rx, &appsink]).unwrap();

        if let Some(bus) = receiver.bus() {
            let _ = bus.add_watch_local(move |_bus, msg| {
                use gst::MessageView;
                match msg.view() {
                    MessageView::Warning(w) => {
                        println!("[receiver][WARN] {} | {:?}", w.error(), w.debug());
                    }
                    MessageView::Error(e) => {
                        println!("[receiver][ERROR] {} | {:?}", e.error(), e.debug());
                    }
                    _ => {}
                }
                glib::ControlFlow::Continue
            });
        }

    (sender, receiver, ristsink)
    };

    // Broadcast mode: duplicates to all links
    let (sender_bcast, receiver_bcast, ristsink_bcast) = build_pipelines("broadcast");
    receiver_bcast.set_state(gst::State::Playing).unwrap();
    sleep(Duration::from_millis(500)).await;
    sender_bcast.set_state(gst::State::Playing).unwrap();
    println!("Broadcast mode running...");

    let mut total_bcast = 0u64;
    for i in 1..=5u64 {
        sleep(Duration::from_secs(1)).await;
        // Print session-stats to confirm all links carry data
        if let Some(stats) = ristsink_bcast.property::<Option<gst::Structure>>("stats") {
            if let Ok(arr) = stats.get::<glib::ValueArray>("session-stats") {
                let mut per = Vec::new();
                for (idx, v) in arr.iter().enumerate() {
                    if let Ok(s) = v.get::<gst::Structure>() { per.push((idx, s.get::<u64>("sent-original-packets").unwrap_or(0))); }
                }
                println!("[bcast t={}s] per-session sent: {:?}", i, per);
            }
            total_bcast = stats.get::<u64>("sent-original-packets").unwrap_or(total_bcast);
        }
    }

    let _ = sender_bcast.set_state(gst::State::Null);
    let _ = receiver_bcast.set_state(gst::State::Null);
    sleep(Duration::from_millis(300)).await;

    // Round-robin mode: distributes across links
    let (sender_rr, receiver_rr, ristsink_rr) = build_pipelines("round-robin");
    receiver_rr.set_state(gst::State::Playing).unwrap();
    sleep(Duration::from_millis(500)).await;
    sender_rr.set_state(gst::State::Playing).unwrap();
    println!("Round-robin mode running...");

    let mut total_rr = 0u64;
    for i in 1..=5u64 {
        sleep(Duration::from_secs(1)).await;
        if let Some(stats) = ristsink_rr.property::<Option<gst::Structure>>("stats") {
            if let Ok(arr) = stats.get::<glib::ValueArray>("session-stats") {
                let mut per = Vec::new();
                for (idx, v) in arr.iter().enumerate() {
                    if let Ok(s) = v.get::<gst::Structure>() { per.push((idx, s.get::<u64>("sent-original-packets").unwrap_or(0))); }
                }
                println!("[rr t={}s] per-session sent: {:?}", i, per);
            }
            total_rr = stats.get::<u64>("sent-original-packets").unwrap_or(total_rr);
        }
    }

    // Quick sanity: total sent should be > 0 in both modes
    println!("Totals: broadcast={}, round-robin={}", total_bcast, total_rr);
    assert!(total_bcast > 200 && total_rr > 200);

    let _ = sender_rr.set_state(gst::State::Null);
    let _ = receiver_rr.set_state(gst::State::Null);
    sleep(Duration::from_millis(300)).await;

    // guards drop to cleanup veth/qdisc
}

#[cfg(not(feature = "network-sim"))]
#[test]
fn quad_links_broadcast_and_roundrobin_fallback() {
    println!("Quad-link tests require the 'network-sim' feature.");
}
