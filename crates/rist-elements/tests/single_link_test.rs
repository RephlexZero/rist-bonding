//! Single-link RIST point-to-point smoke test (no bonding/dispatcher)
//!
//! Builds one veth pair with network-sim and sends a 1080p60 H.265 RTP stream
//! over RIST from sender to receiver. Confirms packet flow by reading ristsink
//! stats. No dispatcher or bonding is used.

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
use std::io::Write;

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
struct VethGuard {
    qdisc: Arc<QdiscManager>,
    veth_tx: String,
    veth_rx: String,
}

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
async fn single_link_point_to_point_rist() {
    init_for_tests();

    // Ensure RIST elements are present
    if gst::ElementFactory::make("ristsink").build().is_err() || gst::ElementFactory::make("ristsrc").build().is_err() {
        println!("Skipping: RIST elements not available (install gst-plugins-bad with RIST)");
        return;
    }

    if !has_net_admin().await {
        println!("Skipping: requires NET_ADMIN to create veth and configure qdisc");
        return;
    }

    println!("=== Single-link RIST P2P Test (1080p60 H.265) ===");

    // Unique veth names to avoid clashes in parallel runs (<= 15 chars)
    let nonce = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().subsec_nanos() % 10000) as u16;
    let veth_tx = format!("vethsS{nonce}");
    let veth_rx = format!("vethrS{nonce}");
    let tx_ip = "10.201.0.1";
    let rx_ip = "10.201.0.2";
    let port: u32 = 5600;

    // Link shaping params generous enough for test bitrate
    let params = NetworkParams { delay_ms: 20, jitter_ms: 0, loss_pct: 0.001, loss_corr_pct: 0.0, duplicate_pct: 0.0, reorder_pct: 0.0, rate_kbps: 5000 };

    let qdisc = Arc::new(QdiscManager::new());
    let _guard = match VethGuard::setup(&veth_tx, &veth_rx, tx_ip, rx_ip, &params, qdisc.clone()).await {
        Ok(g) => g,
        Err(e) => {
            eprintln!("❌ Network setup failed: {e}");
            return;
        }
    };
    println!("Link: {}({}) -> {}({}) @ {} kbps, {}ms, {:.2}% loss", veth_tx, tx_ip, veth_rx, rx_ip, params.rate_kbps, params.delay_ms, params.loss_pct*100.0);

    // Sender pipeline: videotestsrc -> x265enc -> h265parse -> rtph265pay -> ristsink
    let sender = gst::Pipeline::new();
    let videotestsrc = gst::ElementFactory::make("videotestsrc")
        .property("is-live", true)
        .property_from_str("pattern", "smpte")
        .build().unwrap();
    let videoconvert = gst::ElementFactory::make("videoconvert").build().unwrap();
    let capsfilter = gst::ElementFactory::make("capsfilter")
        .property("caps", gst::Caps::builder("video/x-raw")
            .field("format", "I420")
            .field("width", 1920i32)
            .field("height", 1080i32)
            .field("framerate", gst::Fraction::new(60, 1))
            .build())
        .build().unwrap();
    let x265enc = gst::ElementFactory::make("x265enc")
        .property("bitrate", 1500u32)
        .property_from_str("speed-preset", "ultrafast")
        .property_from_str("tune", "zerolatency")
        .build().unwrap();
    let h265parse = gst::ElementFactory::make("h265parse").build().unwrap();
    let rtph265pay = gst::ElementFactory::make("rtph265pay").build().unwrap();
    let ristsink = gst::ElementFactory::make("ristsink")
        .property("address", rx_ip)
        .property("port", port)
        .property("sender-buffer", 1000u32)
        .property("stats-update-interval", 500u32)
        .build().unwrap();
    sender.add_many([&videotestsrc, &videoconvert, &capsfilter, &x265enc, &h265parse, &rtph265pay, &ristsink]).unwrap();
    gst::Element::link_many([&videotestsrc, &videoconvert, &capsfilter, &x265enc, &h265parse, &rtph265pay, &ristsink]).unwrap();

    // Receiver pipeline: ristsrc -> rtph265depay -> h265parse -> avdec_h265 -> videoconvert -> appsink
    let receiver = gst::Pipeline::new();
    let ristsrc = gst::ElementFactory::make("ristsrc")
        .property("address", "0.0.0.0")
        .property("port", port)
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

    // Minimal receiver bus logging (WARN/ERROR)
    if let Some(bus) = receiver.bus() {
        let _ = bus.add_watch_local(move |_bus, msg| {
            use gst::MessageView;
            match msg.view() {
                MessageView::Warning(w) => {
                    let err = w.error();
                    let dbg = w.debug().unwrap_or_else(|| glib::GString::from("<none>"));
                    println!("[receiver][WARN] {} | {:?}", err, dbg);
                }
                MessageView::Error(e) => {
                    let err = e.error();
                    let dbg = e.debug().unwrap_or_else(|| glib::GString::from("<none>"));
                    println!("[receiver][ERROR] {} | {:?}", err, dbg);
                }
                _ => {}
            }
            glib::ControlFlow::Continue
        });
    }

    // Start pipelines (receiver first)
    receiver.set_state(gst::State::Playing).unwrap();
    sleep(Duration::from_millis(500)).await;
    sender.set_state(gst::State::Playing).unwrap();
    sleep(Duration::from_millis(1500)).await;

    // Monitor for a few seconds and print ristsink stats
    println!("Monitoring single-link RIST flow...");
    let duration_secs = 10u64;
    for i in 0..duration_secs {
        sleep(Duration::from_secs(1)).await;
        if let Some(stats) = ristsink.property::<Option<gst::Structure>>("stats") {
            let sent = stats.get::<u64>("sent-original-packets").unwrap_or(0);
            let rtx = stats.get::<u64>("sent-retransmitted-packets").unwrap_or(0);
            print!("t={:>2}s | sent={} rtx={}", i + 1, sent, rtx);
            // Try to show RTT for this single session if present
            if let Ok(session_stats) = stats.get::<glib::ValueArray>("session-stats") {
                if let Some(sv) = session_stats.iter().next() {
                    if let Ok(s) = sv.get::<gst::Structure>() {
                        let rtt = s.get::<u64>("round-trip-time").unwrap_or(0);
                        print!(" rtt={}us", rtt);
                    }
                }
            }
            println!();
        }
        std::io::stdout().flush().ok();
    }

    // Basic assertion: some traffic must have flowed
    let mut packets = 0u64;
    if let Some(stats) = ristsink.property::<Option<gst::Structure>>("stats") {
        packets = stats.get::<u64>("sent-original-packets").unwrap_or(0);
    }
    println!("Total original packets sent: {}", packets);
    assert!(packets > 200, "Expected at least 200 packets sent over single link");

    // Teardown
    let _ = sender.set_state(gst::State::Null);
    let _ = receiver.set_state(gst::State::Null);
    sleep(Duration::from_millis(300)).await;

    println!("✅ Single-link RIST test completed");
}

// Fallback when network-sim isn’t enabled
#[cfg(not(feature = "network-sim"))]
#[test]
fn single_link_point_to_point_rist_fallback() {
    println!("Single link test requires the 'network-sim' feature.");
}
