//! Standalone demo of the new netns-testbench functionality
//! This demonstrates the new Linux namespace-based network simulator

use netns_testbench::{cellular::CellularProfile, LinkHandle as ScenarioHandle, NetworkOrchestrator};
use scenarios::{DirectionSpec, LinkSpec, Schedule, TestScenario};
use std::path::{Path, PathBuf};
use std::time::Duration;

// GStreamer
use gstreamer as gst;
use gst::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing for better debugging
    tracing_subscriber::fmt::init();

    println!("Network Namespace Simulation Demo");
    println!("=====================================\n");

    // Check for required capabilities
    if std::env::var("EUID").unwrap_or_else(|_| "1000".to_string()) != "0" {
        println!("Warning: This demo requires root privileges (CAP_NET_ADMIN)");
        println!("   Run with: sudo -E cargo run --features netns-sim --bin test_netns_demo");
        println!("   or set CAP_NET_ADMIN capability on the binary\n");
    }

    // Initialize the network orchestrator
    let mut orchestrator = NetworkOrchestrator::new(12345).await?;
    println!("Network namespace orchestrator initialized");

    // Build a single scenario consisting of four links with varying bandwidth and loss
    // We'll keep conditions constant and let the network namespace enforce loss/bandwidth.
    let scenario = {
        let mk_dir = |name: &str, kbps: u32, loss: f32| -> LinkSpec {
            let spec = DirectionSpec {
                base_delay_ms: 20,
                jitter_ms: 5,
                loss_pct: loss,
                loss_burst_corr: 0.2,
                reorder_pct: 0.002,
                duplicate_pct: 0.0,
                rate_kbps: kbps,
                mtu: Some(1500),
            };
            LinkSpec::symmetric(
                name.to_string(),
                format!("{}-tx", name),
                format!("{}-rx", name),
                Schedule::Constant(spec),
            )
        };

        let links = vec![
            mk_dir("l0", 800, 0.01),  // 0.01% loss, ~0.8 Mbps
            mk_dir("l1", 1500, 0.02), // 0.02% loss, 1.5 Mbps
            mk_dir("l2", 3000, 0.05), // 0.05% loss, 3.0 Mbps
            mk_dir("l3", 5000, 0.10), // 0.10% loss, 5.0 Mbps
        ];

        TestScenario {
            name: "bond_four_links_constant".into(),
            description: "Four bonded links with constant bandwidth and loss (netns enforced)".into(),
            links,
            duration_seconds: Some(30),
            metadata: Default::default(),
        }
    };

    let rx_port = 7000; // RIST receiver port (even recommended but our element can bind multiple)

    println!("\nStarting 4-link bonding scenario: {}", scenario.name);
    let handles = orchestrator
        .start_bonding_scenarios(vec![
            TestScenario { ..scenario.clone() },
            TestScenario { ..scenario.clone() },
            TestScenario { ..scenario.clone() },
            TestScenario { ..scenario.clone() },
        ], rx_port)
        .await;

    let handles = match handles {
        Ok(h) => h,
        Err(e) => {
            println!("✗ Failed to start bonding scenarios: {}", e);
            if e.to_string().contains("permission") {
                println!("   → Try running with sudo or CAP_NET_ADMIN capability");
            }
            return Err(e.into());
        }
    };

    for (i, h) in handles.iter().enumerate() {
        println!(
            "  Link {}: ingress {} -> egress {} (rx: {})",
            i, h.ingress_port, h.egress_port, h.rx_port
        );
        // quick reachability poke
        let _ = test_link_connectivity(h).await;
    }

    // Apply cellular profiles to each link (mix LTE/5G, light/heavy)
    // Map: l0->lte_light, l1->nr5g_light, l2->lte_heavy, l3->nr5g_heavy
    // The link_id pattern is "link_N" where N starts at 1 in the orchestrator
    let link_ids: Vec<String> = handles.iter().map(|h| h.link_id.clone()).collect();
    for (idx, link_id) in link_ids.iter().enumerate() {
        let profile = match idx {
            0 => CellularProfile::lte_light(),
            1 => CellularProfile::nr5g_light(),
            2 => CellularProfile::lte_heavy(),
            _ => CellularProfile::nr5g_heavy(),
        };
        if let Err(e) = orchestrator.apply_cellular_profile(link_id, &profile).await {
            eprintln!("Failed to apply cellular profile to {}: {}", link_id, e);
        }
    }

    // Display active links summary
    let active_links = orchestrator.get_active_links();
    println!("\nActive Links Summary:");
    println!("   Total links: {}", active_links.len());

    for (i, link) in active_links.iter().enumerate() {
        println!(
            "   Link {}: {} -> {} (rx: {})",
            i + 1,
            link.ingress_port,
            link.egress_port,
            link.rx_port
        );
    }

    // Build bonded ports list from link handles
    let bonded_ports: Vec<u16> = handles.iter().map(|h| h.ingress_port).collect();
    println!("Using bonded ingress ports: {:?}", bonded_ports);

    // Initialize GStreamer
    gst::init()?;

    // Build sender and receiver pipelines
    let (sender, ristsink) = build_sender_pipeline(&bonded_ports)?;
    let output_path = artifact_path("demo_bonded_output.mp4");
    let (receiver, ristsrc) = build_receiver_mp4_pipeline(&bonded_ports, &output_path)?;

    // Start pipelines (receiver first)
    receiver.set_state(gst::State::Playing)?;
    tokio::time::sleep(Duration::from_millis(800)).await;
    sender.set_state(gst::State::Playing)?;

    // Run for 30 seconds while monitoring for errors
    run_and_finalize(sender, receiver, ristsink, ristsrc, Duration::from_secs(30)).await?;

    println!("\nDemo complete. MP4 written to: {}", output_path.display());

    println!("\nNetwork namespace simulation demo completed!");
    println!("   The new netns testbench provides:");
    println!("   • Real Linux network namespaces with veth interfaces");
    println!("   • Configurable qdisc-based traffic shaping and impairments");
    println!("   • Enhanced 4G/5G network behavior modeling");
    println!("   • Time-varying network conditions");
    println!("   • Drop-in replacement for the old network-sim backend");

    println!("Demo completed, orchestrator will clean up on drop");

    Ok(())
}

async fn test_link_connectivity(handle: &ScenarioHandle) -> Result<(), Box<dyn std::error::Error>> {
    // Simple connectivity test - try to bind to the ports to verify they're accessible
    use tokio::net::UdpSocket;

    let test_socket = UdpSocket::bind(("127.0.0.1", 0)).await?;
    let test_data = b"test packet";

    // Try to send a test packet to the ingress port
    if let Err(e) = test_socket
        .send_to(test_data, ("127.0.0.1", handle.ingress_port))
        .await
    {
        return Err(format!(
            "Failed to send to ingress port {}: {}",
            handle.ingress_port, e
        )
        .into());
    }

    // Give a moment for packet processing
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    Ok(())
}

// Build the sender pipeline: 1080p60 H.265 + AAC -> MPEG-TS -> RTP(MP2T) -> ristsink (bonded)
fn build_sender_pipeline(ports: &[u16]) -> Result<(gst::Pipeline, gst::Element), Box<dyn std::error::Error>> {
    let pipeline = gst::Pipeline::new();

    // Video test source at 1080p60
    let vsrc = gst::ElementFactory::make("videotestsrc")
        .property("is-live", true)
        .property_from_str("pattern", "smpte")
        .property("num-buffers", 1800i32) // ~30s at 60fps
        .build()?;
    let vconv = gst::ElementFactory::make("videoconvert").build()?;
    let vscale = gst::ElementFactory::make("videoscale").build()?;
    let v_caps = gst::Caps::builder("video/x-raw")
        .field("width", 1920i32)
        .field("height", 1080i32)
        .field("framerate", gst::Fraction::new(60, 1))
        .build();
    let vcap = gst::ElementFactory::make("capsfilter").property("caps", &v_caps).build()?;
    let venc = gst::ElementFactory::make("x265enc")
        .property_from_str("speed-preset", "ultrafast")
        .property_from_str("tune", "zerolatency")
        .property("bitrate", 8000u32) // 8 Mbps target
        .property("key-int-max", 120i32)
        .build()?;
    let vparse = gst::ElementFactory::make("h265parse")
        .property("config-interval", 1i32)
        .build()?;

    // Audio test source: sine at 48kHz, 2ch, ~30s
    let asrc = gst::ElementFactory::make("audiotestsrc")
        .property("is-live", true)
        .property("freq", 440.0f64)
        .property("samplesperbuffer", 4800i32) // 0.1s per buffer at 48kHz
        .property("num-buffers", 300i32) // 300 * 0.1s = 30s
        .build()?;
    let aconv = gst::ElementFactory::make("audioconvert").build()?;
    let ares = gst::ElementFactory::make("audioresample").build()?;
    let a_caps = gst::Caps::builder("audio/x-raw")
        .field("rate", 48000i32)
        .field("channels", 2i32)
        .build();
    let acap = gst::ElementFactory::make("capsfilter").property("caps", &a_caps).build()?;
    let aenc = gst::ElementFactory::make("avenc_aac")
        .property("bitrate", 256000i32)
        .build()?;
    let aparse = gst::ElementFactory::make("aacparse").build()?;

    // Mux to MPEG-TS
    let tsmux = gst::ElementFactory::make("mpegtsmux")
        .property("alignment", 7i32)
        .property("latency", gst::ClockTime::from_mseconds(200))
        .build()?;

    // RTP payload for MP2T
    let rtpmp2tpay = gst::ElementFactory::make("rtpmp2tpay")
        .property("pt", 33u32)
        .build()?;

    // RIST sink configured for bonded addresses
    let ristsink = gst::ElementFactory::make("ristsink")
        .property("sender-buffer", 5000u32)
        .build()?;
    let bonding_addrs = ports
        .iter()
        .map(|p| format!("127.0.0.1:{}", p))
        .collect::<Vec<_>>()
        .join(",");
    ristsink.set_property("bonding-addresses", bonding_addrs);

    // Add and link elements
    pipeline.add_many([
        &vsrc, &vconv, &vscale, &vcap, &venc, &vparse, // video
        &asrc, &aconv, &ares, &acap, &aenc, &aparse,    // audio
        &tsmux, &rtpmp2tpay, &ristsink,
    ])?;

    // Link video branch
    gst::Element::link_many([&vsrc, &vconv, &vscale, &vcap, &venc, &vparse])?;
    // Link audio branch
    gst::Element::link_many([&asrc, &aconv, &ares, &acap, &aenc, &aparse])?;

    // Request and link TS muxer pads (mpegtsmux uses request pads sink_%d)
    let v_pad = vparse.static_pad("src").unwrap();
    let v_sink = tsmux.request_pad_simple("sink_%d").unwrap();
    v_pad.link(&v_sink)?;
    let a_pad = aparse.static_pad("src").unwrap();
    let a_sink = tsmux.request_pad_simple("sink_%d").unwrap();
    a_pad.link(&a_sink)?;

    // Link TS -> RTP -> RIST sink
    gst::Element::link_many([&tsmux, &rtpmp2tpay, &ristsink])?;

    Ok((pipeline, ristsink))
}

// Build the receiver pipeline: ristsrc (bonded) -> rtpmp2tdepay -> tsdemux -> h265parse/aacparse -> mp4mux -> filesink
fn build_receiver_mp4_pipeline(
    ports: &[u16],
    output_path: &Path,
) -> Result<(gst::Pipeline, gst::Element), Box<dyn std::error::Error>> {
    let pipeline = gst::Pipeline::new();

    // ristsrc listening on all bonded ports
    let ristsrc = gst::ElementFactory::make("ristsrc")
        .property("receiver-buffer", 5000u32)
        .property("encoding-name", "MP2T")
        .build()?;
    let bonding_addrs = ports
        .iter()
        .map(|p| format!("0.0.0.0:{}", p))
        .collect::<Vec<_>>()
        .join(",");
    ristsrc.set_property("bonding-addresses", bonding_addrs);

    let rtpmp2tdepay = gst::ElementFactory::make("rtpmp2tdepay").build()?;
    let tsdemux = gst::ElementFactory::make("tsdemux").build()?;

    // Parsers and MP4 mux
    let h265parse = gst::ElementFactory::make("h265parse")
        // Send parameter sets periodically to ensure downstream gets caps before segment
        // This helps avoid "Sticky event misordering (segment before caps)" warnings.
        .property("config-interval", 1i32)
        .build()?;
    let aacparse = gst::ElementFactory::make("aacparse").build()?;
    let vqueue = gst::ElementFactory::make("queue").build()?;
    let aqueue = gst::ElementFactory::make("queue").build()?;
    let mp4mux = gst::ElementFactory::make("mp4mux").build()?;
    let filesink = gst::ElementFactory::make("filesink")
        .property("location", output_path.to_string_lossy().to_string())
        .build()?;

    // Add elements
    pipeline.add_many([
        &ristsrc,
        &rtpmp2tdepay,
        &tsdemux,
        &h265parse,
        &vqueue,
        &aacparse,
        &aqueue,
        &mp4mux,
        &filesink,
    ])?;

    // Static links
    gst::Element::link_many([&ristsrc, &rtpmp2tdepay, &tsdemux])?;
    mp4mux.link(&filesink)?;

    // Link parse queues to mux (dynamic pads used from demux)
    h265parse.link(&vqueue)?;
    aacparse.link(&aqueue)?;

    let mp4mux_clone = mp4mux.clone();
    let h265parse_clone = h265parse.clone();
    let aacparse_clone = aacparse.clone();
    let vqueue_clone = vqueue.clone();
    let aqueue_clone = aqueue.clone();
    tsdemux.connect_pad_added(move |_demux, pad| {
        let name = pad.name();
        if name.starts_with("video_") {
            // demux video -> h265parse -> vqueue -> mp4mux(video)
            let sink_pad = h265parse_clone.static_pad("sink").unwrap();
            if pad.link(&sink_pad).is_err() {
                eprintln!("Failed to link demux video to h265parse");
                return;
            }
            let vq_src = vqueue_clone.static_pad("src").unwrap();
            if let Some(v_pad) = mp4mux_clone.request_pad_simple("video_%u") {
                let _ = vq_src.link(&v_pad);
            }
        } else if name.starts_with("audio_") {
            // demux audio -> aacparse -> aqueue -> mp4mux(audio)
            let sink_pad = aacparse_clone.static_pad("sink").unwrap();
            if pad.link(&sink_pad).is_err() {
                eprintln!("Failed to link demux audio to aacparse");
                return;
            }
            let aq_src = aqueue_clone.static_pad("src").unwrap();
            if let Some(a_pad) = mp4mux_clone.request_pad_simple("audio_%u") {
                let _ = aq_src.link(&a_pad);
            }
        }
    });

    Ok((pipeline, ristsrc))
}

// Run for duration, then gracefully finalize MP4 by sending EOS to receiver and waiting for Eos on the bus
async fn run_and_finalize(
    sender: gst::Pipeline,
    receiver: gst::Pipeline,
    ristsink: gst::Element,
    ristsrc: gst::Element,
    duration: Duration,
) -> Result<(), Box<dyn std::error::Error>> {
    // Helper to format and print RIST stats element messages succinctly
    fn maybe_print_rist_stats(msg: &gst::Message) {
        if let Some(structure) = msg.structure() {
            let name = structure.name().to_ascii_lowercase();
            // Heuristic: only handle messages that look like RIST stats
            if !(name.contains("rist") && name.contains("stat")) {
                return;
            }

            // Try to extract a concise set of fields if available
            // Handle known RIST stat structures explicitly for best readability
            let mut parts: Vec<String> = Vec::new();
            match name.as_str() {
                "rist/x-sender-stats" => {
                    if let Ok(sent) = structure.get::<u64>("sent-original-packets") {
                        parts.push(format!("sent={}", sent));
                    }
                    if let Ok(rtx) = structure.get::<u64>("sent-retransmitted-packets") {
                        parts.push(format!("rtx={}", rtx));
                    }
                    // We don't attempt to expand session-stats array here to keep it concise
                }
                "rist/x-receiver-stats" => {
                    if let Ok(recv) = structure.get::<u64>("received") {
                        parts.push(format!("rx={}", recv));
                    }
                    if let Ok(dropd) = structure.get::<u64>("dropped") {
                        parts.push(format!("drop={}", dropd));
                    }
                    if let Ok(dups) = structure.get::<u64>("duplicates") {
                        parts.push(format!("dup={}", dups));
                    }
                    if let Ok(req) = structure.get::<u64>("retransmission-requests-sent") {
                        parts.push(format!("rtx_req={}", req));
                    }
                    if let Ok(rtt_us) = structure.get::<u64>("rtx-roundtrip-time") {
                        let rtt_ms = (rtt_us as f64) / 1000.0;
                        parts.push(format!("rtt={:.0}ms", rtt_ms));
                    }
                }
                // Generic fallback: probe a few optional common fields if present
                _ => {
                    if let Ok(rtt) = structure.get::<i64>("rtt_ms") {
                        parts.push(format!("rtt={}ms", rtt));
                    } else if let Ok(rttf) = structure.get::<f64>("rtt_ms") {
                        parts.push(format!("rtt={:.0}ms", rttf));
                    }
                    if let Ok(loss) = structure.get::<f64>("loss_pct") {
                        parts.push(format!("loss={:.2}%", loss));
                    } else if let Ok(lossi) = structure.get::<i64>("loss_pct") {
                        parts.push(format!("loss={}%", lossi));
                    }
                    if let Ok(in_kbps) = structure.get::<i64>("in_rate_kbps") {
                        parts.push(format!("in={} kbps", in_kbps));
                    }
                    if let Ok(out_kbps) = structure.get::<i64>("out_rate_kbps") {
                        parts.push(format!("out={} kbps", out_kbps));
                    }
                    if let Ok(n) = structure.get::<i32>("session_count") {
                        parts.push(format!("sessions={}", n));
                    }
                }
            }

            // Element name if present
            if let Some(src) = msg.src() {
                let elem_name = src.name();
                if parts.is_empty() {
                    // Fallback to just structure name to avoid giant dumps
                    println!("[{}] {}", elem_name, structure.name());
                } else {
                    println!("[{}] {}: {}", elem_name, structure.name(), parts.join(", "));
                }
            } else if parts.is_empty() {
                println!("{}", structure.name());
            } else {
                println!("{}: {}", structure.name(), parts.join(", "));
            }
        }
    }

    // Helper to format and print dynbitrate messages
    fn maybe_print_dynbitrate_stats(msg: &gst::Message) {
        if let Some(structure) = msg.structure() {
            let name = structure.name().to_ascii_lowercase();
            if name == "dynbitrate/current-bitrate" {
                if let Ok(bitrate_kbps) = structure.get::<u32>("bitrate-kbps") {
                    if let Some(src) = msg.src() {
                        println!("[{}] Current encoder bitrate: {} kbps", src.name(), bitrate_kbps);
                    } else {
                        println!("Current encoder bitrate: {} kbps", bitrate_kbps);
                    }
                }
            }
        }
    }

    let start = tokio::time::Instant::now();
    let mut last_stats = tokio::time::Instant::now() - Duration::from_secs(10);
    while start.elapsed() < duration {
        // Every ~1s, fetch concise stats from ristsink and ristsrc and print them
        if last_stats.elapsed() >= Duration::from_secs(1) {
            // Helper to query a Structure-valued "stats" property and print concise fields
            let print_stats = |elem: &gst::Element| {
                let val = elem.property_value("stats");
                if let Ok(structure) = val.get::<gst::Structure>() {
                    let name = structure.name().to_ascii_lowercase();
                    let mut parts: Vec<String> = Vec::new();
                    match name.as_str() {
                        "rist/x-sender-stats" => {
                            if let Ok(sent) = structure.get::<u64>("sent-original-packets") {
                                parts.push(format!("sent={}", sent));
                            }
                            if let Ok(rtx) = structure.get::<u64>("sent-retransmitted-packets") {
                                parts.push(format!("rtx={}", rtx));
                            }
                        }
                        "rist/x-receiver-stats" => {
                            if let Ok(recv) = structure.get::<u64>("received") {
                                parts.push(format!("rx={}", recv));
                            }
                            if let Ok(dropd) = structure.get::<u64>("dropped") {
                                parts.push(format!("drop={}", dropd));
                            }
                            if let Ok(dups) = structure.get::<u64>("duplicates") {
                                parts.push(format!("dup={}", dups));
                            }
                            if let Ok(req) = structure.get::<u64>("retransmission-requests-sent") {
                                parts.push(format!("rtx_req={}", req));
                            }
                            if let Ok(rtt_us) = structure.get::<u64>("rtx-roundtrip-time") {
                                let rtt_ms = (rtt_us as f64) / 1000.0;
                                parts.push(format!("rtt={:.0}ms", rtt_ms));
                            }
                        }
                        _ => {}
                    }
                    if !parts.is_empty() {
                        println!("[{}] {}: {}", elem.name(), structure.name(), parts.join(", "));
                    }
                }
            };

            print_stats(&ristsink);
            print_stats(&ristsrc);
            last_stats = tokio::time::Instant::now();
        }

        // Check for errors on both buses
        if let Some(bus) = sender.bus() {
            while let Some(msg) = bus.pop() {
                match msg.view() {
                    gst::MessageView::Error(err) => {
                        let _ = sender.set_state(gst::State::Null);
                        let _ = receiver.set_state(gst::State::Null);
                        return Err(format!(
                            "Sender error: {} - {}",
                            err.error(),
                            err.debug().unwrap_or_else(|| "".into())
                        )
                        .into());
                    }
                    gst::MessageView::Element(_) => {
                        maybe_print_rist_stats(&msg);
                        maybe_print_dynbitrate_stats(&msg);
                    }
                    _ => {}
                }
            }
        }
        if let Some(bus) = receiver.bus() {
            while let Some(msg) = bus.pop() {
                match msg.view() {
                    gst::MessageView::Error(err) => {
                        let _ = sender.set_state(gst::State::Null);
                        let _ = receiver.set_state(gst::State::Null);
                        return Err(format!(
                            "Receiver error: {} - {}",
                            err.error(),
                            err.debug().unwrap_or_else(|| "".into())
                        )
                        .into());
                    }
                    gst::MessageView::Element(_) => {
                        maybe_print_rist_stats(&msg);
                        maybe_print_dynbitrate_stats(&msg);
                    }
                    _ => {}
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Stop sender, then EOS receiver to finalize MP4
    sender.set_state(gst::State::Null)?;
    let _ = receiver.send_event(gst::event::Eos::new());

    // Wait up to 10 seconds for EOS
    if let Some(bus) = receiver.bus() {
        let timeout = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            if let Some(msg) = bus.timed_pop(gst::ClockTime::from_mseconds(200)) {
                if let gst::MessageView::Eos(..) = msg.view() {
                    break;
                }
                if let gst::MessageView::Error(err) = msg.view() {
                    eprintln!(
                        "Receiver error while waiting for EOS: {} - {}",
                        err.error(),
                        err.debug().unwrap_or_else(|| "".into())
                    );
                    break;
                }
            }
            if std::time::Instant::now() > timeout {
                eprintln!("Timed out waiting for EOS on receiver");
                break;
            }
        }
    }

    receiver.set_state(gst::State::Null)?;
    Ok(())
}

// Resolve an artifacts path under target/test-artifacts
fn artifact_path(file_name: &str) -> PathBuf {
    let base = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("target"));
    let dir = base.join("test-artifacts");
    let _ = std::fs::create_dir_all(&dir);
    dir.join(file_name)
}
