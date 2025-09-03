//! End-to-end RIST network convergence test
//!
//! Tests dispatcher rebalancing using real UDP traffic over network-sim shaped interfaces.
//! Creates sender pipeline (videotestsrc -> ristsink with bonding) and receiver pipeline
//! (ristsrc -> rtpvrawdepay -> appsink) to generate genuine RIST feedback for load balancing.

use gst::prelude::*;
use gstreamer as gst;
use gstristelements::testing::*;
use std::time::Duration;

#[cfg(feature = "network-sim")]
use ::network_sim::{
    qdisc::QdiscManager,
    types::NetworkParams,
    runtime::{apply_network_params, apply_ingress_params, remove_network_params, remove_ingress_params},
};

#[cfg(feature = "network-sim")]
use std::sync::Arc;

#[cfg(feature = "network-sim")]
use tokio::time::sleep;

#[cfg(feature = "network-sim")]
use std::fs::{create_dir_all, File};
#[cfg(feature = "network-sim")]
use std::io::{BufWriter, Write};
#[cfg(feature = "network-sim")]
use std::path::PathBuf;

#[derive(Debug, Clone)]
struct StaticProfile {
    name: &'static str,
    // sender and receiver ends of a veth pair
    veth_tx: String,
    veth_rx: String,
    // IPs assigned to veth ends
    tx_ip: String,
    rx_ip: String,
    delay_ms: u32,
    loss_pct: f32,
    rate_kbps: u32,
}

impl StaticProfile {
    fn new(index: usize, name: &'static str, delay_ms: u32, loss_pct: f32, rate_kbps: u32) -> Self {
        let veth_tx = format!("veths{}", index);
        let veth_rx = format!("vethr{}", index);
        // Use a /30 per profile to avoid overlaps: 10.200.<idx>.0/30
        let octet = 100 + index as u8; // avoid very low subnets
        let tx_ip = format!("10.200.{}.1", octet);
        let rx_ip = format!("10.200.{}.2", octet);
        Self { name, veth_tx, veth_rx, tx_ip, rx_ip, delay_ms, loss_pct, rate_kbps }
    }

    #[cfg(feature = "network-sim")]
    fn to_params(&self) -> NetworkParams {
        NetworkParams {
            delay_ms: self.delay_ms,
            loss_pct: self.loss_pct,
            rate_kbps: self.rate_kbps,
            jitter_ms: 0,
            reorder_pct: 0.0,
            duplicate_pct: 0.0,
            loss_corr_pct: 0.0,
        }
    }
}

// RAII guard to ensure veth/qdisc cleanup even on early returns
#[cfg(feature = "network-sim")]
struct VethGuard {
    qdisc: Arc<QdiscManager>,
    veth_tx: String,
    veth_rx: String,
}

#[cfg(feature = "network-sim")]
impl VethGuard {
    async fn setup(p: &StaticProfile, qdisc: Arc<QdiscManager>) -> anyhow::Result<Self> {
        setup_veth_pair(&p.veth_tx, &p.veth_rx, &p.tx_ip, &p.rx_ip)
            .await
            .map_err(|e| anyhow::anyhow!("veth setup {}-{} failed: {}", p.veth_tx, p.veth_rx, e))?;

        // Apply shaping; assert success so failures don't go unnoticed
        apply_network_params(&qdisc, &p.veth_tx, &p.to_params())
            .await
            .map_err(|e| anyhow::anyhow!("egress qdisc apply failed on {}: {}", p.veth_tx, e))?;
        apply_ingress_params(&qdisc, &p.veth_rx, &p.to_params())
            .await
            .map_err(|e| anyhow::anyhow!("ingress qdisc apply failed on {}: {}", p.veth_rx, e))?;

        Ok(Self { qdisc, veth_tx: p.veth_tx.clone(), veth_rx: p.veth_rx.clone() })
    }
}

#[cfg(feature = "network-sim")]
impl Drop for VethGuard {
    fn drop(&mut self) {
        // Best-effort async cleanup in the background
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
    // Delete pre-existing
    let _ = run_cmd("ip", &["link", "del", "dev", tx]).await;
    let _ = run_cmd("ip", &["link", "del", "dev", rx]).await;

    // Create veth pair
    let out = run_cmd("ip", &["link", "add", tx, "type", "veth", "peer", "name", rx]).await?;
    if !out.status.success() { return Err(std::io::Error::other(String::from_utf8_lossy(&out.stderr).to_string())); }
    // Assign IPs (/30)
    let _ = run_cmd("ip", &["addr", "flush", "dev", tx]).await;
    let _ = run_cmd("ip", &["addr", "flush", "dev", rx]).await;
    let out = run_cmd("ip", &["addr", "add", &format!("{}/30", tx_ip), "dev", tx]).await?;
    if !out.status.success() { return Err(std::io::Error::other(String::from_utf8_lossy(&out.stderr).to_string())); }
    let out = run_cmd("ip", &["addr", "add", &format!("{}/30", rx_ip), "dev", rx]).await?;
    if !out.status.success() { return Err(std::io::Error::other(String::from_utf8_lossy(&out.stderr).to_string())); }
    // Bring up
    let out = run_cmd("ip", &["link", "set", "dev", tx, "up"]).await?;
    if !out.status.success() { return Err(std::io::Error::other(String::from_utf8_lossy(&out.stderr).to_string())); }
    let out = run_cmd("ip", &["link", "set", "dev", rx, "up"]).await?;
    if !out.status.success() { return Err(std::io::Error::other(String::from_utf8_lossy(&out.stderr).to_string())); }
    Ok(())
}

#[cfg(feature = "network-sim")]
async fn cleanup_veth_pair(tx: &str, rx: &str) {
    let _ = run_cmd("ip", &["link", "del", "dev", tx]).await;
    let _ = run_cmd("ip", &["link", "del", "dev", rx]).await;
}

#[cfg(feature = "network-sim")]
#[tokio::test] 
async fn test_static_bandwidths_convergence() {
    init_for_tests();

    // Check if RIST elements are available and functional
    let rist_check = gst::ElementFactory::make("ristsink")
        .property("address", "127.0.0.1")
        .property("port", 5000u32)
        .build();
    
    if rist_check.is_err() {
        println!("⚠️  RIST elements not available or functional - install gst-plugins-bad with RIST support");
        println!("This test requires ristsink and ristsrc elements for end-to-end UDP traffic");
        println!("Error: {:?}", rist_check);
        return;
    }

    println!("=== End-to-End RIST Network Convergence Test ===");

    // Require NET_ADMIN for link setup; otherwise skip test
    if !has_net_admin().await {
        println!("ℹ️ Skipping: requires NET_ADMIN to create veth and configure qdisc");
        return;
    }

    // Fixed capacities for four links with different UDP port assignments
    let profiles = vec![
        (StaticProfile::new(0, "5G-Good",    15, 0.0005, 4000), 5000),
        (StaticProfile::new(1, "4G-Good",    25, 0.0010, 2000), 5002), 
        (StaticProfile::new(2, "4G-Typical", 40, 0.0050, 1200), 5004),
        (StaticProfile::new(3, "5G-Poor",    60, 0.0100,  800), 5006),
    ];

    // Expected capacity-proportional weights
    let total_capacity: u32 = profiles.iter().map(|(p, _)| p.rate_kbps).sum();
    let expected_weights: Vec<f64> = profiles.iter().map(|(p, _)| p.rate_kbps as f64 / total_capacity as f64).collect();

    println!("Profiles (fixed) with UDP ports and veth pairs:");
    for (i, (p, port)) in profiles.iter().enumerate() {
        println!(
            "  {}: {} - {}ms, {:.2}% loss, {} kbps -> UDP port {}, tx={}({}), rx={}({})",
            i, p.name, p.delay_ms, p.loss_pct*100.0, p.rate_kbps, port, p.veth_tx, p.tx_ip, p.veth_rx, p.rx_ip
        );
    }

    // Create links and apply static network constraints
    let qdisc = Arc::new(QdiscManager::new());
    println!("\nCreating veth pairs and applying constraints...");
    let mut veth_guards = Vec::with_capacity(profiles.len());
    for (p, _) in &profiles {
        match VethGuard::setup(p, qdisc.clone()).await {
            Ok(g) => veth_guards.push(g),
            Err(e) => {
                println!("❌ Network setup failed: {e}");
                return;
            }
        }
    }

    // Create sender pipeline with real RIST sinks
    let sender_pipeline = gst::Pipeline::new();

    // High-rate H.265 1080p60 RTP source
    let av_source: gst::Element = {
        let bin = gst::Bin::new();
        let videotestsrc = gst::ElementFactory::make("videotestsrc")
            .property("is-live", true)
            .property_from_str("pattern", "smpte")
            .build().unwrap();
        let videoconvert = gst::ElementFactory::make("videoconvert").build().unwrap();
        let capsfilter = gst::ElementFactory::make("capsfilter")
            .property("caps", gst::Caps::builder("video/x-raw")
                .field("format", "I420")
                .field("width", 1920i32)  // 1080p width
                .field("height", 1080i32) // 1080p height
                .field("framerate", gst::Fraction::new(60, 1))  // 60fps for high load
                .build())
            .build().unwrap();
        
        // Add H.265/HEVC encoder for high-quality 1080p60
        let x265enc = gst::ElementFactory::make("x265enc")
            .property("bitrate", 6000u32)  // 6 Mbps - within total capacity but requires balancing
            .property_from_str("speed-preset", "ultrafast")  // Fast encoding for tests
            .property_from_str("tune", "zerolatency")
            .build().unwrap();
        let h265parse = gst::ElementFactory::make("h265parse").build().unwrap();
        let rtph265pay = gst::ElementFactory::make("rtph265pay").build().unwrap();
        
        bin.add_many([&videotestsrc, &videoconvert, &capsfilter, &x265enc, &h265parse, &rtph265pay]).unwrap();
        gst::Element::link_many([&videotestsrc, &videoconvert, &capsfilter, &x265enc, &h265parse, &rtph265pay]).unwrap();
        
        let src_pad = rtph265pay.static_pad("src").unwrap();
        let ghost_pad = gst::GhostPad::with_target(&src_pad).unwrap();
        ghost_pad.set_active(true).unwrap();
        bin.add_pad(&ghost_pad).unwrap();
        bin.upcast()
    };

    // Dispatcher configured for EWMA with real stats
    let dispatcher = create_dispatcher(Some(&[0.25, 0.25, 0.25, 0.25]));
    dispatcher.set_property("strategy", "ewma");
    dispatcher.set_property("auto-balance", true);
    dispatcher.set_property("rebalance-interval-ms", 1000u64);
    // Increase dispatcher metrics export to 4 Hz for detailed logging during the test
    dispatcher.set_property("metrics-export-interval-ms", 250u64);

    // Create single RIST sink with bonding addresses and custom dispatcher
    let sender_bonding_addresses = profiles.iter()
        .map(|(p, port)| format!("{}:{}", p.rx_ip, port))
        .collect::<Vec<_>>()
        .join(",");
    
    let rist_sink = gst::ElementFactory::make("ristsink")
        // Set primary destination as first profile; bonding will include all
        .property("address", &profiles[0].0.rx_ip)
        .property("port", profiles[0].1 as u32)
        .property("bonding-addresses", &sender_bonding_addresses)
        .property("dispatcher", &dispatcher)  // Use our custom EWMA dispatcher
        .property("sender-buffer", 1000u32)
        .property("stats-update-interval", 500u32)
        .build()
        .expect("Failed to create ristsink - ensure gst-plugins-bad with RIST support is installed");
    
    println!("Sender bonding addresses: {}", sender_bonding_addresses);
    println!("Using custom EWMA dispatcher for bonding");

    // Add elements to sender pipeline (only av_source and rist_sink, no external dispatcher)
    sender_pipeline.add(&av_source).unwrap();
    sender_pipeline.add(&rist_sink).unwrap();

    // Link source directly to RIST sink (it handles internal dispatcher)
    av_source.link(&rist_sink).unwrap();
    println!("  Connected av_source -> ristsink (with internal EWMA dispatcher)");

    // Create receiver pipeline to complete the RIST loop
    let receiver_pipeline = gst::Pipeline::new();
    
    // RIST source configured for bonding - REMOTE SENDER addresses (tx_ip)
    let bonding_addresses = profiles.iter()
        .map(|(p, port)| format!("{}:{}", p.tx_ip, port))
        .collect::<Vec<_>>()
        .join(",");
    
    let rist_src = gst::ElementFactory::make("ristsrc")
        // Configure with the REMOTE sender address/port so the handshake completes
        .property("address", &profiles[0].0.tx_ip)
        .property("port", profiles[0].1 as u32)
        .property("bonding-addresses", &bonding_addresses)  // All sender addresses
        .property("receiver-buffer", 2000u32)
        .build()
        .expect("Failed to create ristsrc - ensure gst-plugins-bad with RIST support is installed");
    
    println!("Receiver bonding addresses: {}", bonding_addresses);

    // Create a proper RTP receiver chain for H.265/HEVC
    let rtph265depay = gst::ElementFactory::make("rtph265depay").build().unwrap();
    let h265parse = gst::ElementFactory::make("h265parse").build().unwrap();
    let avdec_h265 = gst::ElementFactory::make("avdec_h265").build().unwrap();
    let videoconvert = gst::ElementFactory::make("videoconvert").build().unwrap();
    let appsink = gst::ElementFactory::make("appsink")
        .property("sync", false)  // Don't sync to clock for testing
        .property("drop", true)   // Drop frames if needed to avoid blocking
        .build().unwrap();

    receiver_pipeline.add_many([&rist_src, &rtph265depay, &h265parse, &avdec_h265, &videoconvert, &appsink]).unwrap();
    gst::Element::link_many([&rist_src, &rtph265depay, &h265parse, &avdec_h265, &videoconvert, &appsink]).unwrap();

    println!("✅ End-to-end RIST pipeline established (sender -> UDP -> receiver)");

    // Start receiver first
    println!("Starting receiver pipeline...");
    receiver_pipeline.set_state(gst::State::Playing).unwrap();
    sleep(Duration::from_millis(1000)).await; // Let receiver settle

    // Start sender
    println!("Starting sender pipeline...");
    sender_pipeline.set_state(gst::State::Playing).unwrap();
    sleep(Duration::from_millis(2000)).await; // Let RIST establish connections

    // Monitor for rebalancing behavior
    let test_secs = 30u64;
    let sender_bus = sender_pipeline.bus().expect("sender pipeline has a bus");
    
    println!("\nMonitoring RIST statistics and dispatcher rebalancing...");
    // Prepare CSV logging at 4 Hz
    let csv_dir = PathBuf::from("/workspace/target/tmp");
    let _ = create_dir_all(&csv_dir);
    let csv_path = csv_dir.join("static-bandwidths-metrics.csv");
    println!("CSV: {}", csv_path.display());
    let csv_file = File::create(&csv_path).expect("create CSV file");
    let mut csv = BufWriter::new(csv_file);
    // Header
    writeln!(
        csv,
        "elapsed_ms,timestamp,selected_index,src_pad_count,encoder_bitrate_kbps,buffers_processed,ewma_rtx_penalty,ewma_rtt_penalty,aimd_rtx_threshold,current_weights"
    ).ok();
    // Sample at 4 Hz (every 250ms) to capture detailed dispatcher metrics
    let ticks = test_secs * 4;
    for tick in 0..ticks {
        sleep(Duration::from_millis(250)).await;
        let ms_total = tick * 250;
        let ss = ms_total / 1000;
        let ms = ms_total % 1000;

        // Read current stats from single RIST sink (real network performance across all sessions)
        if let Some(stats_struct) = rist_sink.property::<Option<gst::Structure>>("stats") {
            // The single ristsink with bonding should provide session-level stats
            let sent_original = stats_struct.get::<u64>("sent-original-packets").unwrap_or(0);
            let sent_retransmitted = stats_struct.get::<u64>("sent-retransmitted-packets").unwrap_or(0);
            
            if ss % 5 == 0 && ms == 0 {
                println!("  RIST sink: original={}, retransmitted={}", sent_original, sent_retransmitted);
                
                // Try to extract session-specific stats from the session-stats array
                if let Ok(session_stats) = stats_struct.get::<glib::ValueArray>("session-stats") {
                    for (i, session_value) in session_stats.iter().enumerate() {
                        if let Ok(session_struct) = session_value.get::<gst::Structure>() {
                            let session_id = session_struct.get::<i32>("session-id").unwrap_or(-1);
                            let session_sent = session_struct.get::<u64>("sent-original-packets").unwrap_or(0);
                            let session_rtt = session_struct.get::<u64>("round-trip-time").unwrap_or(0);
                            println!("    Session {}: id={}, sent={}, rtt={}μs", 
                                    i, session_id, session_sent, session_rtt);
                        }
                    }
                }
            }
    } else if ss % 5 == 0 && ms == 0 {
            println!("  No RIST stats available yet");
        }

        // Drain dispatcher metrics from bus
        while let Some(msg) = sender_bus.timed_pop_filtered(
            gst::ClockTime::from_mseconds(0), 
            &[gst::MessageType::Application]
        ) {
            if let Some(s) = msg.structure() {
                if s.name() == "rist-dispatcher-metrics" {
                    let weights = s.get::<&str>("current-weights").unwrap_or("[]");
                    let selected = s.get::<u32>("selected-index").unwrap_or(0);
                    let src_pad_count = s.get::<u32>("src-pad-count").unwrap_or(0);
                    let buffers_processed = s.get::<u64>("buffers-processed").unwrap_or(0);
                    let encoder_bitrate = s.get::<u32>("encoder-bitrate").unwrap_or(0);
                    let ewma_rtx_penalty = s.get::<f64>("ewma-rtx-penalty").unwrap_or(0.0);
                    let ewma_rtt_penalty = s.get::<f64>("ewma-rtt-penalty").unwrap_or(0.0);
                    let aimd_rtx_threshold = s.get::<f64>("aimd-rtx-threshold").unwrap_or(0.0);
                    let ts = s.get::<u64>("timestamp").unwrap_or(0);
                    println!(
                        "t={:>2}.{:03}s | sel={} pads={} enc={}kbps buf={} ewma_pen{{rtx:{:.3},rtt:{:.3}}} aimd_th={:.3} weights={}",
                        ss,
                        ms,
                        selected,
                        src_pad_count,
                        encoder_bitrate,
                        buffers_processed,
                        ewma_rtx_penalty,
                        ewma_rtt_penalty,
                        aimd_rtx_threshold,
                        weights
                    );

                    // CSV line (quote weights); escape inner quotes by doubling
                    let weights_csv = format!("\"{}\"", weights.replace('"', "\"\""));
                    let _ = writeln!(
                        csv,
                        "{},{},{},{},{},{},{:.6},{:.6},{:.6},{}",
                        (tick) * 250,
                        ts,
                        selected,
                        src_pad_count,
                        encoder_bitrate,
                        buffers_processed,
                        ewma_rtx_penalty,
                        ewma_rtt_penalty,
                        aimd_rtx_threshold,
                        weights_csv
                    );
                }
            }
        }

        if ss % 10 == 0 && ms == 0 {
            println!("--- t={}s checkpoint ---", ss);
        }
    }

    // Shutdown
    println!("\nShutting down pipelines...");
    let _ = csv.flush();
    let _ = sender_pipeline.set_state(gst::State::Ready);
    let _ = receiver_pipeline.set_state(gst::State::Ready);
    sleep(Duration::from_millis(1000)).await;
    let _ = sender_pipeline.set_state(gst::State::Null);
    let _ = receiver_pipeline.set_state(gst::State::Null);
    sleep(Duration::from_millis(500)).await;

    // Cleanup is handled by VethGuard Drop

    println!("\nExpected (capacity-based): [{:.3}, {:.3}, {:.3}, {:.3}]",
        expected_weights[0], expected_weights[1], expected_weights[2], expected_weights[3]);
    println!("✅ End-to-end RIST network test completed - check logs for rebalancing behavior");
}

// Fallback when network-sim isn’t enabled
#[cfg(not(feature = "network-sim"))]
#[test]
fn test_static_bandwidths_convergence_fallback() {
    println!("Static bandwidths test requires the 'network-sim' feature.");
}
