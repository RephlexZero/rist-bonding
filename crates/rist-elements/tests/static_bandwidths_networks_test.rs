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
    get_connection_ips,
    namespace::{cleanup_rist_test_links, create_shaped_veth_pair, ShapedVethConfig},
    qdisc::QdiscManager,
    runtime::{apply_ingress_params, remove_ingress_params},
    types::NetworkParams,
};

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
        Self {
            name,
            veth_tx,
            veth_rx,
            tx_ip,
            rx_ip,
            delay_ms,
            loss_pct,
            rate_kbps,
        }
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

// network-sim capability check
#[cfg(feature = "network-sim")]
async fn has_net_admin() -> bool {
    QdiscManager::new().has_net_admin().await
}

#[cfg(feature = "network-sim")]
#[tokio::test]
async fn test_static_bandwidths_convergence() {
    // Reduce GStreamer debug noise unless user explicitly set GST_DEBUG
    if std::env::var_os("GST_DEBUG").is_none() {
        // Suppress RTX timing warnings while keeping important dispatcher info
        std::env::set_var(
            "GST_DEBUG",
            "ristdispatcher:INFO,ristrtxsend:ERROR,*:WARNING",
        );
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
        (StaticProfile::new(0, "5G-Good", 15, 0.0005, 1500), 5000),
        (StaticProfile::new(1, "4G-Good", 25, 0.0010, 1250), 5002),
        (StaticProfile::new(2, "4G-Typical", 40, 0.0050, 750), 5004),
        (StaticProfile::new(3, "5G-Poor", 60, 0.0100, 300), 5006),
    ];

    // Expected capacity-proportional weights
    let total_capacity: u32 = profiles.iter().map(|(p, _)| p.rate_kbps).sum();
    let expected_weights: Vec<f64> = profiles
        .iter()
        .map(|(p, _)| p.rate_kbps as f64 / total_capacity as f64)
        .collect();

    println!("Profiles (fixed) with UDP ports and veth pairs:");
    for (i, (p, port)) in profiles.iter().enumerate() {
        println!(
            "  {}: {} - {}ms, {:.2}% loss, {} kbps -> UDP port {}, tx={}({}), rx={}({})",
            i,
            p.name,
            p.delay_ms,
            p.loss_pct * 100.0,
            p.rate_kbps,
            port,
            p.veth_tx,
            p.tx_ip,
            p.veth_rx,
            p.rx_ip
        );
    }

    // Create links and apply static network constraints using network-sim APIs
    let qdisc = QdiscManager::new();
    println!("\nCreating shaped veth pairs and applying constraints (network-sim)...");
    let mut link_configs: Vec<ShapedVethConfig> = Vec::with_capacity(profiles.len());
    for (p, _) in &profiles {
        let cfg = ShapedVethConfig {
            tx_interface: p.veth_tx.clone(),
            rx_interface: p.veth_rx.clone(),
            tx_ip: format!("{}/30", p.tx_ip),
            rx_ip: format!("{}/30", p.rx_ip),
            rx_namespace: None,
            network_params: p.to_params(),
        };
        if let Err(e) = create_shaped_veth_pair(&qdisc, &cfg).await {
            panic!("Network setup failed for {}-{}: {}", cfg.tx_interface, cfg.rx_interface, e);
        }
        // Optionally shape ingress on RX to model downstream constraints
        if let Err(e) = apply_ingress_params(&qdisc, &cfg.rx_interface, &cfg.network_params).await {
            eprintln!("Warning: ingress apply failed on {}: {}", cfg.rx_interface, e);
        }
        link_configs.push(cfg);
    }

    // Create sender pipeline with real RIST sinks
    let sender_pipeline = gst::Pipeline::new();

    // Raw video RTP source (~5000 kbps): I420 140x100 @ 30 fps
    let av_source: gst::Element = {
        let bin = gst::Bin::new();
        let videotestsrc = gst::ElementFactory::make("videotestsrc")
            .property("is-live", true)
            .property_from_str("pattern", "smpte")
            .property("do-timestamp", true) // Ensure proper timestamps
            .build()
            .unwrap();
        let videoconvert = gst::ElementFactory::make("videoconvert").build().unwrap();
    let capsfilter = gst::ElementFactory::make("capsfilter")
            .property(
                "caps",
                gst::Caps::builder("video/x-raw")
            .field("format", "I420")
            .field("width", 140i32)
            .field("height", 100i32)
            .field("framerate", gst::Fraction::new(30, 1))
                    .build(),
            )
            .build()
            .unwrap();
        // RTP payloader for raw video
        let rtpvrawpay = gst::ElementFactory::make("rtpvrawpay")
            .property("mtu", 1200u32)
            // Set payload type and SSRC on the payloader instead of forcing caps downstream
            .property("pt", 96u32)
            .property("ssrc", 0x0022u32)
            .build()
            .unwrap();

        bin.add_many([
            &videotestsrc,
            &videoconvert,
            &capsfilter,
            &rtpvrawpay,
        ])
        .unwrap();
        gst::Element::link_many([
            &videotestsrc,
            &videoconvert,
            &capsfilter,
            &rtpvrawpay,
        ])
        .unwrap();

        let src_pad = rtpvrawpay.static_pad("src").unwrap();
        let ghost_pad = gst::GhostPad::with_target(&src_pad).unwrap();
        ghost_pad.set_active(true).unwrap();
        bin.add_pad(&ghost_pad).unwrap();
        bin.upcast()
    };

    // Dispatcher configured for EWMA with real stats
    let dispatcher = create_dispatcher(Some(&[0.25, 0.25, 0.25, 0.25]));
    dispatcher.set_property("strategy", "ewma");
    dispatcher.set_property("auto-balance", true);
    // Use SWRR (smooth weighted round robin) scheduler which has better burst behavior than DRR
    dispatcher.set_property("scheduler", "swrr");
    dispatcher.set_property("quantum-bytes", 1200u32); // align with RTP MTU
                                                       // Loosen stickiness slightly and speed up early rebalances
    dispatcher.set_property("min-hold-ms", 200u64);
    dispatcher.set_property("switch-threshold", 1.05f64);
    dispatcher.set_property("ewma-rtx-penalty", 0.30f64);
    dispatcher.set_property("ewma-rtt-penalty", 0.10f64);
    dispatcher.set_property("rebalance-interval-ms", 500u64);
    // Increase dispatcher metrics export to 4 Hz for detailed logging during the test
    dispatcher.set_property("metrics-export-interval-ms", 250u64);

    // Create single RIST sink with bonding addresses and custom dispatcher
    // IMPORTANT: Append "/<ifname>" to each address to force SO_BINDTODEVICE on the udpsink sockets.
    // ristsink parses "address:port[/iface]" and sets the child udpsink "multicast-iface" property,
    // which triggers SO_BINDTODEVICE in multiudpsink and ensures egress via the specified veth device.
    let sender_bonding_addresses = link_configs
        .iter()
        .zip(profiles.iter().map(|(_, port)| *port))
        .map(|(cfg, port)| {
            let (_tx_ip, rx_ip) = get_connection_ips(cfg);
            format!("{}:{}/{}", rx_ip, port, cfg.tx_interface)
        })
        .collect::<Vec<_>>()
        .join(",");

    let rist_sink = gst::ElementFactory::make("ristsink")
        // Set primary destination as first profile; bonding will include all
    .property("address", get_connection_ips(&link_configs[0]).1)
    .property("port", profiles[0].1 as u32)
        .property("bonding-addresses", &sender_bonding_addresses)
        // Also set the top-level multicast-iface so the primary bond (session 0) is bound early
    .property("multicast-iface", &link_configs[0].tx_interface)
        .property("dispatcher", &dispatcher) // Use our custom EWMA dispatcher
    // Speed up RTCP so we see RR/RTT signals sooner
    .property("min-rtcp-interval", 50u32)
        .property("sender-buffer", 1000u32)
        .property("stats-update-interval", 500u32)
        .property("multicast-loopback", false)
        .build()
        .expect(
            "Failed to create ristsink - ensure gst-plugins-bad with RIST support is installed",
        );

    println!("Sender bonding addresses: {}", sender_bonding_addresses);
    println!("Using custom EWMA dispatcher for bonding");

    // Add elements to sender pipeline and insert a capsfilter to set a fixed RTP SSRC
    sender_pipeline.add(&av_source).unwrap();
    // Insert a queue to decouple upstream from ristsink
    let queue = gst::ElementFactory::make("queue").build().unwrap();
    sender_pipeline.add(&queue).unwrap();
    sender_pipeline.add(&rist_sink).unwrap();

    // Link source -> queue -> RIST sink (payloader already sets PT/SSRC)
    gst::Element::link_many([&av_source, &queue, &rist_sink]).unwrap();
    println!(
        "  Connected av_source -> capsfilter(ssrc) -> ristsink (with internal EWMA dispatcher)"
    );

    // Create receiver pipeline to complete the RIST loop
    let receiver_pipeline = gst::Pipeline::new();

    // RIST source configured for bonding - listen on RECEIVER addresses (rx_ip)
    // Append "/<ifname>" to bind the receiving sockets to the veth RX device as well.
    let bonding_addresses = link_configs
        .iter()
        .zip(profiles.iter().map(|(_, port)| *port))
        .map(|(cfg, port)| {
            let (_tx_ip, rx_ip) = get_connection_ips(cfg);
            format!("{}:{}/{}", rx_ip, port, cfg.rx_interface)
        })
        .collect::<Vec<_>>()
        .join(",");

    let rist_src = gst::ElementFactory::make("ristsrc")
        // Bind to first receiver address for primary connection
    .property("address", get_connection_ips(&link_configs[0]).1)
    .property("port", profiles[0].1 as u32)
        .property("bonding-addresses", &bonding_addresses) // All receiver addresses
        // Ensure primary bond binds to the RX veth device early as well
    .property("multicast-iface", &link_configs[0].rx_interface)
    // Help internal rtpbin map dynamic PT=96 to RAW caps
    .property("encoding-name", "RAW")
        .property("receiver-buffer", 2000u32)
    // Speed up RR emission to improve feedback
    .property("min-rtcp-interval", 50u32)
        .property("multicast-loopback", false)
        .build()
        .expect("Failed to create ristsrc - ensure gst-plugins-bad with RIST support is installed");

    println!("Receiver bonding addresses: {}", bonding_addresses);

    // Create a proper RTP receiver chain for H.265/HEVC with jitter buffer
    let rtpjitterbuffer = gst::ElementFactory::make("rtpjitterbuffer")
        .property("latency", 200u32) // Match RTP latency
        .property("drop-on-latency", true) // Drop late packets
        .build()
        .unwrap();
    let rtpvrawdepay = gst::ElementFactory::make("rtpvrawdepay").build().unwrap();
    let videoconvert = gst::ElementFactory::make("videoconvert").build().unwrap();
    let appsink = gst::ElementFactory::make("appsink")
        .property("sync", false) // Don't sync to clock for testing
        .property("drop", true) // Drop frames if needed to avoid blocking
        .build()
        .unwrap();

    receiver_pipeline
        .add_many([
            &rist_src,
            &rtpjitterbuffer,
            &rtpvrawdepay,
            &videoconvert,
            &appsink,
        ])
        .unwrap();
    gst::Element::link_many([
        &rist_src,
        &rtpjitterbuffer,
        &rtpvrawdepay,
        &videoconvert,
        &appsink,
    ])
    .unwrap();

    println!("✅ End-to-end RIST pipeline established (sender -> UDP -> receiver)");

    // Set up synchronized clocks for both pipelines to avoid timestamp issues
    let system_clock = gst::SystemClock::obtain();
    let _ = sender_pipeline.set_clock(Some(&system_clock));
    let _ = receiver_pipeline.set_clock(Some(&system_clock));

    // Set base time to ensure synchronized timestamps
    let base_time = system_clock.time();
    sender_pipeline.set_base_time(base_time);
    receiver_pipeline.set_base_time(base_time);

    // Configure video source for better timing - higher frame rate and larger buffers
    if let Some(videotestsrc) = sender_pipeline.by_name("video_src") {
        videotestsrc.set_property("is-live", true);
        videotestsrc.set_property("do-timestamp", true);
        videotestsrc.set_property_from_str("pattern", "smpte");
        // Ensure adequate frame rate to avoid "source too slow" warnings
        println!("  Configured videotestsrc for live mode with timestamps");
    }

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
                        if let Some(rtpsession) =
                            rtpbin_bin.by_name(&format!("rtpsession{}", session_id))
                        {
                            rtpsession.set_property_from_str("ntp-time-source", "running-time");
                            println!(
                                "    Set sender rtpsession{} ntp-time-source to running-time",
                                session_id
                            );
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
                        if let Some(rtpsession) =
                            rtpbin_bin.by_name(&format!("rtpsession{}", session_id))
                        {
                            rtpsession.set_property_from_str("ntp-time-source", "running-time");
                            println!(
                                "    Set receiver rtpsession{} ntp-time-source to running-time",
                                session_id
                            );
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
    writeln!(
        weights_csv,
        "elapsed_ms,timestamp,session_0_weight,session_1_weight,session_2_weight,session_3_weight"
    )
    .ok();

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
            let sent_original = stats_struct
                .get::<u64>("sent-original-packets")
                .unwrap_or(0);
            let sent_retransmitted = stats_struct
                .get::<u64>("sent-retransmitted-packets")
                .unwrap_or(0);
            // Print a compact one-line summary every 5s
            if ss % 5 == 0 && ms == 0 {
                let mut per = String::new();
                if let Ok(session_stats) = stats_struct.get::<glib::ValueArray>("session-stats") {
                    let mut parts: Vec<String> = Vec::with_capacity(session_stats.len());
                    for session_value in session_stats.iter() {
                        if let Ok(session_struct) = session_value.get::<gst::Structure>() {
                            let id = session_struct.get::<i32>("session-id").unwrap_or(-1);
                            let sent = session_struct
                                .get::<u64>("sent-original-packets")
                                .unwrap_or(0);
                            let rtt = session_struct.get::<u64>("round-trip-time").unwrap_or(0);
                            parts.push(format!("id{}:sent{} rtt{}us", id, sent, rtt));
                        }
                    }
                    per = parts.join(" | ");
                }
                println!(
                    "  RIST sink: orig={} retrx={} | {}",
                    sent_original, sent_retransmitted, per
                );

                // CSV logging unchanged
                if let Ok(session_stats) = stats_struct.get::<glib::ValueArray>("session-stats") {
                    for session_value in session_stats.iter() {
                        if let Ok(session_struct) = session_value.get::<gst::Structure>() {
                            let session_id = session_struct.get::<i32>("session-id").unwrap_or(-1);
                            let session_sent = session_struct
                                .get::<u64>("sent-original-packets")
                                .unwrap_or(0);
                            let session_retrans = session_struct
                                .get::<u64>("sent-retransmitted-packets")
                                .unwrap_or(0);
                            let session_rtt =
                                session_struct.get::<u64>("round-trip-time").unwrap_or(0);
                            let _ = writeln!(
                                sessions_csv,
                                "{},{},{},{},{},{},0.0,0.0",
                                ms_total,
                                SystemTime::now()
                                    .duration_since(UNIX_EPOCH)
                                    .unwrap()
                                    .as_millis(),
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
                                    SystemTime::now()
                                        .duration_since(UNIX_EPOCH)
                                        .unwrap()
                                        .as_millis(),
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
            &[gst::MessageType::Application],
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
                                w0,
                                w1,
                                w2,
                                w3
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
    for w in per_session_csvs.iter_mut() {
        let _ = w.flush();
    }
    let _ = sender_pipeline.set_state(gst::State::Ready);
    let _ = receiver_pipeline.set_state(gst::State::Ready);
    sleep(Duration::from_millis(1000)).await;
    let _ = sender_pipeline.set_state(gst::State::Null);
    let _ = receiver_pipeline.set_state(gst::State::Null);
    sleep(Duration::from_millis(500)).await;

    // Cleanup shaped links (remove ingress and delete links)
    for cfg in &link_configs {
        let _ = remove_ingress_params(&qdisc, &cfg.rx_interface).await;
    }
    if let Err(e) = cleanup_rist_test_links(&qdisc, &link_configs).await {
        eprintln!("Cleanup warning: {}", e);
    }

    println!(
        "\nExpected (capacity-based): [{:.3}, {:.3}, {:.3}, {:.3}]",
        expected_weights[0], expected_weights[1], expected_weights[2], expected_weights[3]
    );

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

    println!(
        "✅ Convergence within tolerance (max_abs_err {:.3} <= {:.3})",
        max_abs_err, tol
    );
}

// Fallback when network-sim isn’t enabled
#[cfg(not(feature = "network-sim"))]
#[test]
fn test_static_bandwidths_convergence_fallback() {
    println!("Static bandwidths test requires the 'network-sim' feature.");
}
