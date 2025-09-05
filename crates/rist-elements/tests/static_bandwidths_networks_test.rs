//! End-to-end RIST network convergence test
//!
//! Tests dispatcher rebalancing using real UDP traffic over network-sim shaped interfaces.
//! Creates sender pipeline (videotestsrc -> ristsink with bonding) and receiver pipeline
//! (ristsrc -> rtpvrawdepay -> appsink) to generate genuine RIST feedback for load balancing.

use gst::prelude::*;
use gstreamer as gst;
use gstristelements::testing::*;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
    // Reduce GStreamer debug noise unless user explicitly set GST_DEBUG
    if std::env::var_os("GST_DEBUG").is_none() {
        // WARNING level: suppress INFO/DEBUG element dumps
        std::env::set_var("GST_DEBUG", "WARNING");
    }

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
                panic!("Network setup failed: {e}");
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
            .property("do-timestamp", true)  // Ensure proper timestamps
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
            .property("bitrate", 3000u32)  // Lower to reduce backpressure and ensure flow
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
        .property("multicast-loopback", false)
        .build()
        .expect("Failed to create ristsink - ensure gst-plugins-bad with RIST support is installed");
    
    println!("Sender bonding addresses: {}", sender_bonding_addresses);
    println!("Using custom EWMA dispatcher for bonding");

    // Add elements to sender pipeline (only av_source and rist_sink)
    sender_pipeline.add(&av_source).unwrap();
    sender_pipeline.add(&rist_sink).unwrap();

    // Link source directly to RIST sink (it handles internal dispatcher)
    av_source.link(&rist_sink).unwrap();
    println!("  Connected av_source -> ristsink (with internal EWMA dispatcher)");

    // Create receiver pipeline to complete the RIST loop
    let receiver_pipeline = gst::Pipeline::new();
    
    // RIST source configured for bonding - listen on RECEIVER addresses (rx_ip)
    let bonding_addresses = profiles.iter()
        .map(|(p, port)| format!("{}:{}", p.rx_ip, port))
        .collect::<Vec<_>>()
        .join(",");
    
    let rist_src = gst::ElementFactory::make("ristsrc")
        // Bind to first receiver address for primary connection
        .property("address", &profiles[0].0.rx_ip)
        .property("port", profiles[0].1 as u32)
        .property("bonding-addresses", &bonding_addresses)  // All receiver addresses
        // Provide explicit RTP caps with clock-rate to avoid jitter buffer warnings
        .property("caps", gst::Caps::builder("application/x-rtp")
            .field("media", "video")
            .field("encoding-name", "H265")
            .field("clock-rate", 90000i32)  // Standard video clock rate
            .field("payload", 96i32)        // Dynamic payload type
            .build())
        .property("receiver-buffer", 2000u32)
        .property("multicast-loopback", false)
        .build()
        .expect("Failed to create ristsrc - ensure gst-plugins-bad with RIST support is installed");
    
    println!("Receiver bonding addresses: {}", bonding_addresses);

    // Create a proper RTP receiver chain for H.265/HEVC with jitter buffer
    let rtpjitterbuffer = gst::ElementFactory::make("rtpjitterbuffer")
        .property("latency", 200u32)         // Match RTP latency
        .property("drop-on-latency", true)   // Drop late packets
        .build().unwrap();
    let rtph265depay = gst::ElementFactory::make("rtph265depay").build().unwrap();
    let h265parse = gst::ElementFactory::make("h265parse").build().unwrap();
    let avdec_h265 = gst::ElementFactory::make("avdec_h265").build().unwrap();
    let videoconvert = gst::ElementFactory::make("videoconvert").build().unwrap();
    let appsink = gst::ElementFactory::make("appsink")
        .property("sync", false)  // Don't sync to clock for testing
        .property("drop", true)   // Drop frames if needed to avoid blocking
        .build().unwrap();

    receiver_pipeline.add_many([&rist_src, &rtpjitterbuffer, &rtph265depay, &h265parse, &avdec_h265, &videoconvert, &appsink]).unwrap();
    gst::Element::link_many([&rist_src, &rtpjitterbuffer, &rtph265depay, &h265parse, &avdec_h265, &videoconvert, &appsink]).unwrap();

    println!("✅ End-to-end RIST pipeline established (sender -> UDP -> receiver)");

    // Set up synchronized clocks for both pipelines to avoid timestamp issues
    let system_clock = gst::SystemClock::obtain();
    let _ = sender_pipeline.set_clock(Some(&system_clock));
    let _ = receiver_pipeline.set_clock(Some(&system_clock));
    
    // Set base time to ensure synchronized timestamps
    let base_time = system_clock.time();
    sender_pipeline.set_base_time(base_time);
    receiver_pipeline.set_base_time(base_time);
    
    // Configure RTP latency on internal rtpbin elements to fix timing warnings
    let configure_rtpbin_latency = |element: &gst::Element, latency_ms: u32| {
        // RIST elements are bins, so we can access their internal elements
        if let Ok(bin) = element.clone().downcast::<gst::Bin>() {
            if let Some(rtpbin) = bin.by_name("rist_send_rtpbin") {
                rtpbin.set_property("latency", latency_ms);
                rtpbin.set_property("drop-on-latency", true);
                rtpbin.set_property("do-sync-event", true);
                println!("  Configured sender rtpbin latency: {}ms", latency_ms);
                
                // Configure internal rtpsession elements to use running-time for RTCP
                if let Ok(rtpbin_bin) = rtpbin.downcast::<gst::Bin>() {
                    for session_id in 0..4 {
                        if let Some(rtpsession) = rtpbin_bin.by_name(&format!("rtpsession{}", session_id)) {
                            rtpsession.set_property_from_str("ntp-time-source", "running-time");
                            println!("    Set sender rtpsession{} ntp-time-source to running-time", session_id);
                        } else {
                            println!("    Sender rtpsession{} not found", session_id);
                        }
                    }
                }
            }
            if let Some(rtpbin) = bin.by_name("rist_recv_rtpbin") {
                rtpbin.set_property("latency", latency_ms);
                rtpbin.set_property("drop-on-latency", true);
                rtpbin.set_property("do-sync-event", true);
                println!("  Configured receiver rtpbin latency: {}ms", latency_ms);
                
                // Configure receiver rtpsession elements
                if let Ok(rtpbin_bin) = rtpbin.downcast::<gst::Bin>() {
                    for session_id in 0..4 {
                        if let Some(rtpsession) = rtpbin_bin.by_name(&format!("rtpsession{}", session_id)) {
                            rtpsession.set_property_from_str("ntp-time-source", "running-time");
                            println!("    Set receiver rtpsession{} ntp-time-source to running-time", session_id);
                        } else {
                            println!("    Receiver rtpsession{} not found", session_id);
                        }
                    }
                }
            }
        }
    };
    
    // Start pipelines first, then configure rtpbin latency after state change
    println!("Starting receiver pipeline...");
    receiver_pipeline.set_state(gst::State::Playing).unwrap();
    
    println!("Starting sender pipeline...");
    sender_pipeline.set_state(gst::State::Playing).unwrap();
    
    // Wait for pipelines to reach playing state before configuring latency
    sleep(Duration::from_millis(500)).await;
    
    // Now configure latency on both RIST elements after they're playing
    configure_rtpbin_latency(&rist_sink, 200);
    configure_rtpbin_latency(&rist_src, 200);

    // Wait longer for all rtpsession elements to be created before accessing them
    sleep(Duration::from_millis(3000)).await; // Let RIST establish connections
    
    // Try configuring rtpsession elements again after more time
    configure_rtpbin_latency(&rist_sink, 200);
    configure_rtpbin_latency(&rist_src, 200);

    // Monitor for rebalancing behavior
    let test_secs = 30u64;
    let sender_bus = sender_pipeline.bus().expect("sender pipeline has a bus");
    
    println!("\nMonitoring RIST statistics and dispatcher rebalancing...");
    // Prepare CSV logging at 4 Hz
    let csv_dir = PathBuf::from("/workspace/target/tmp");
    let _ = create_dir_all(&csv_dir);
    let csv_path = csv_dir.join("static-bandwidths-metrics.csv");
    println!("CSV: {}", csv_path.display());
    // Create separate CSV files for better analysis
    let base_path = "/workspace/target/tmp/static-bandwidths";
    let weights_path = format!("{}-weights.csv", base_path);
    let sessions_path = format!("{}-sessions.csv", base_path);
    let metrics_path = format!("{}-metrics.csv", base_path);
    // Per-session CSVs (one file per session id)
    let mut per_session_csvs: Vec<BufWriter<File>> = Vec::new();
    for sid in 0..profiles.len() {
        let p = format!("{}-sessions-{}.csv", base_path, sid);
        let f = File::create(&p).expect("create per-session CSV file");
        let mut w = BufWriter::new(f);
        // Same schema as aggregate sessions CSV
        let _ = writeln!(w, "elapsed_ms,timestamp,session_id,sent_original_packets,sent_retransmitted_packets,rtt_us,goodput_pps,rtx_rate");
        per_session_csvs.push(w);
    }
    
    // Weights CSV - tracks weight evolution over time
    let weights_file = File::create(&weights_path).expect("create weights CSV file");
    let mut weights_csv = BufWriter::new(weights_file);
    writeln!(weights_csv, "elapsed_ms,timestamp,session_0_weight,session_1_weight,session_2_weight,session_3_weight").ok();
    
    // Sessions CSV - individual session stats over time
    let sessions_file = File::create(&sessions_path).expect("create sessions CSV file");
    let mut sessions_csv = BufWriter::new(sessions_file);
    writeln!(sessions_csv, "elapsed_ms,timestamp,session_id,sent_original_packets,sent_retransmitted_packets,rtt_us,goodput_pps,rtx_rate").ok();
    
    // Metrics CSV - dispatcher-level metrics over time
    let metrics_file = File::create(&metrics_path).expect("create metrics CSV file");
    let mut metrics_csv = BufWriter::new(metrics_file);
    writeln!(metrics_csv, "elapsed_ms,timestamp,selected_index,src_pad_count,encoder_bitrate_kbps,buffers_processed,ewma_rtx_penalty,ewma_rtt_penalty,aimd_rtx_threshold").ok();
    
    println!("CSV files:");
    println!("  Weights: {}", weights_path);
    println!("  Sessions: {}", sessions_path);
    println!("  Metrics: {}", metrics_path);
    for sid in 0..per_session_csvs.len() {
        println!("  Session {}: {}-sessions-{}.csv", sid, base_path, sid);
    }
    // Sample at 4 Hz (every 250ms) to capture detailed dispatcher metrics
    let ticks = test_secs * 4;
    // Track the latest weights observed from dispatcher metrics
    let mut last_weights: Option<Vec<f64>> = None;
    let main_ctx = glib::MainContext::default();
    for tick in 0..ticks {
        sleep(Duration::from_millis(250)).await;
        // Pump GLib main loop so timeout_add callbacks (e.g., metrics export) can run
        // Drain all pending events without blocking
        while main_ctx.iteration(false) {}
        let ms_total = tick * 250;
        let ss = ms_total / 1000;
        let ms = ms_total % 1000;

        // Read current stats from single RIST sink (real network performance across all sessions)
        if let Some(stats_struct) = rist_sink.property::<Option<gst::Structure>>("stats") {
            // The single ristsink with bonding should provide session-level stats
            let sent_original = stats_struct.get::<u64>("sent-original-packets").unwrap_or(0);
            let sent_retransmitted = stats_struct.get::<u64>("sent-retransmitted-packets").unwrap_or(0);
            // Print a compact one-line summary every 5s
            if ss % 5 == 0 && ms == 0 {
                let mut per = String::new();
                if let Ok(session_stats) = stats_struct.get::<glib::ValueArray>("session-stats") {
                    let mut parts: Vec<String> = Vec::with_capacity(session_stats.len());
                    for session_value in session_stats.iter() {
                        if let Ok(session_struct) = session_value.get::<gst::Structure>() {
                            let id = session_struct.get::<i32>("session-id").unwrap_or(-1);
                            let sent = session_struct.get::<u64>("sent-original-packets").unwrap_or(0);
                            let rtt = session_struct.get::<u64>("round-trip-time").unwrap_or(0);
                            parts.push(format!("id{}:sent{} rtt{}us", id, sent, rtt));
                        }
                    }
                    per = parts.join(" | ");
                }
                println!("  RIST sink: orig={} retrx={} | {}", sent_original, sent_retransmitted, per);

                // CSV logging unchanged
                if let Ok(session_stats) = stats_struct.get::<glib::ValueArray>("session-stats") {
                    for session_value in session_stats.iter() {
                        if let Ok(session_struct) = session_value.get::<gst::Structure>() {
                            let session_id = session_struct.get::<i32>("session-id").unwrap_or(-1);
                            let session_sent = session_struct.get::<u64>("sent-original-packets").unwrap_or(0);
                            let session_retrans = session_struct.get::<u64>("sent-retransmitted-packets").unwrap_or(0);
                            let session_rtt = session_struct.get::<u64>("round-trip-time").unwrap_or(0);
                            let _ = writeln!(
                                sessions_csv,
                                "{},{},{},{},{},{},0.0,0.0",
                                ms_total,
                                SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis(),
                                session_id,
                                session_sent,
                                session_retrans,
                                session_rtt
                            );
                            if session_id >= 0 && (session_id as usize) < per_session_csvs.len() {
                                let _ = writeln!(
                                    per_session_csvs[session_id as usize],
                                    "{},{},{},{},{},{},0.0,0.0",
                                    ms_total,
                                    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis(),
                                    session_id,
                                    session_sent,
                                    session_retrans,
                                    session_rtt
                                );
                            }
                        }
                    }
                }
            }
    } else if ss % 5 == 0 && ms == 0 {
            println!("  RIST sink: no stats yet");
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
                    // Throttle metrics printing to once per second for readability
                    if ms == 0 {
                        println!(
                            "t={:>2}.{:03}s | sel={} pads={} enc={}kbps ewma_rtx={:.3} aimd_th={:.3} weights={}",
                            ss,
                            ms,
                            selected,
                            src_pad_count,
                            encoder_bitrate,
                            ewma_rtx_penalty,
                            aimd_rtx_threshold,
                            weights
                        );
                    }

                    // Write to metrics CSV
                    let _ = writeln!(
                        metrics_csv,
                        "{},{},{},{},{},{},{:.6},{:.6},{:.6}",
                        (tick) * 250,
                        ts,
                        selected,
                        src_pad_count,
                        encoder_bitrate,
                        buffers_processed,
                        ewma_rtx_penalty,
                        ewma_rtt_penalty,
                        aimd_rtx_threshold
                    );

                    // Write to weights CSV
                    if let Ok(v) = serde_json::from_str::<Vec<f64>>(weights) {
                        // Validate finite and normalize if necessary to guard against rounding
                        if !v.is_empty() && v.iter().all(|w| w.is_finite() && *w >= 0.0) {
                            // Ensure we have at least 4 weights, pad with 0.0 if needed
                            let w0 = v.first().copied().unwrap_or(0.0);
                            let w1 = v.get(1).copied().unwrap_or(0.0);
                            let w2 = v.get(2).copied().unwrap_or(0.0);
                            let w3 = v.get(3).copied().unwrap_or(0.0);
                            
                            let _ = writeln!(
                                weights_csv,
                                "{},{},{:.6},{:.6},{:.6},{:.6}",
                                (tick) * 250,
                                ts,
                                w0, w1, w2, w3
                            );
                            
                            last_weights = Some(v);
                        }
                    }
                }
            }
        }

        if ss % 10 == 0 && ms == 0 {
            println!("--- t={}s checkpoint ---", ss);
        }
    }

    // Shutdown
    println!("\nShutting down pipelines...");
    let _ = weights_csv.flush();
    let _ = sessions_csv.flush();
    let _ = metrics_csv.flush();
    for w in per_session_csvs.iter_mut() { let _ = w.flush(); }
    let _ = sender_pipeline.set_state(gst::State::Ready);
    let _ = receiver_pipeline.set_state(gst::State::Ready);
    sleep(Duration::from_millis(1000)).await;
    let _ = sender_pipeline.set_state(gst::State::Null);
    let _ = receiver_pipeline.set_state(gst::State::Null);
    sleep(Duration::from_millis(500)).await;

    // Cleanup is handled by VethGuard Drop

    println!("\nExpected (capacity-based): [{:.3}, {:.3}, {:.3}, {:.3}]",
        expected_weights[0], expected_weights[1], expected_weights[2], expected_weights[3]);

    // Assert convergence: compare the latest observed weights to expected capacity ratios
    // Allow a reasonable tolerance due to measurement noise and encoding variability.
    let observed = last_weights.expect("No dispatcher weights observed; metrics not received");
    assert_eq!(
        observed.len(),
        expected_weights.len(),
        "Observed weights length {} != expected {}",
        observed.len(),
        expected_weights.len()
    );

    // Compute maximum absolute error across links
    let mut max_abs_err = 0.0f64;
    for (i, (obs, exp)) in observed.iter().zip(expected_weights.iter()).enumerate() {
        let err = (obs - exp).abs();
        max_abs_err = max_abs_err.max(err);
        println!(
            "Link {}: observed {:.3}, expected {:.3}, abs_err {:.3}",
            i, obs, exp, err
        );
    }

    // Tolerance: 12 percentage points per link
    let tol = 0.12f64;
    assert!(
        max_abs_err <= tol,
        "Weights did not converge: observed={:?}, expected={:?}, max_abs_err={:.3} > tol={:.3}",
        observed,
        expected_weights,
        max_abs_err,
        tol
    );

    println!("✅ Convergence within tolerance (max_abs_err {:.3} <= {:.3})", max_abs_err, tol);
}

// Fallback when network-sim isn’t enabled
#[cfg(not(feature = "network-sim"))]
#[test]
fn test_static_bandwidths_convergence_fallback() {
    println!("Static bandwidths test requires the 'network-sim' feature.");
}
