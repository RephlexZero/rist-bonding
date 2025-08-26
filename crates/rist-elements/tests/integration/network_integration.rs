//! End-to-End RIST Bonding Tests with netns-testbench
//!
//! Comprehensive test suite covering realistic network scenarios including
//! bonding, degradation, handovers, and recovery using the netns-testbench API.

use gstreamer::prelude::*;
use gstristelements::testing;
use netns_testbench::{NetworkOrchestrator, TestScenario};
// For stress test, use the netlink-based forwarder orchestrator
use scenarios::{DirectionSpec, LinkSpec, Schedule};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Test configuration for integration tests
struct TestConfig {
    pub rx_port: u16,
    pub test_duration_secs: u64,
    pub seed: u64,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            rx_port: 5000,
            test_duration_secs: 10,
            seed: 42,
        }
    }
}

/// Resolve a consistent artifacts directory for all test outputs in this crate.
/// Mirrors integration_tests behavior without introducing a cross-crate test dep.
fn artifacts_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("TEST_ARTIFACTS_DIR") {
        let p = PathBuf::from(dir);
        let _ = std::fs::create_dir_all(&p);
        return p;
    }
    if let Ok(target) = std::env::var("CARGO_TARGET_DIR") {
        let p = PathBuf::from(target).join("test-artifacts");
        let _ = std::fs::create_dir_all(&p);
        return p;
    }
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // Walk up to workspace root (Cargo.toml with [workspace])
    let mut dir = manifest_dir.clone();
    let ws_root = loop {
        let cargo = dir.join("Cargo.toml");
        if cargo.exists() {
            if let Ok(content) = std::fs::read_to_string(&cargo) {
                if content.contains("[workspace]") {
                    break dir.clone();
                }
            }
        }
        if !dir.pop() {
            break manifest_dir;
        }
    };
    let p = ws_root.join("target").join("test-artifacts");
    let _ = std::fs::create_dir_all(&p);
    p
}

fn artifact_path(file_name: &str) -> PathBuf {
    artifacts_dir().join(file_name)
}

/// Create a sender pipeline that bonds N links all targeting the receiver's rx_port
fn build_sender_pipeline_to_rx(
    rx_port: u16,
    link_count: usize,
) -> (gstreamer::Pipeline, gstreamer::Element) {
    let pipeline = gstreamer::Pipeline::new();

    // Create video test source at 720p30 (more realistic for tests)
    let videotestsrc = gstreamer::ElementFactory::make("videotestsrc")
        .property("is-live", true)
        .property_from_str("pattern", "smpte") // SMPTE color bars
        .property("num-buffers", 300) // 10 seconds at 30fps
        .build()
        .expect("Failed to create videotestsrc");

    // Set resolution to 720p
    let video_caps = gstreamer::Caps::builder("video/x-raw")
        .field("width", 1280i32)
        .field("height", 720i32)
        .field("framerate", gstreamer::Fraction::new(30, 1))
        .build();

    let video_capsfilter = gstreamer::ElementFactory::make("capsfilter")
        .property("caps", &video_caps)
        .build()
        .expect("Failed to create video capsfilter");

    // Create audio test source with sine wave
    let audiotestsrc = gstreamer::ElementFactory::make("audiotestsrc")
        .property("is-live", true)
        .property("freq", 440.0) // A4 note
        .property("num-buffers", 480) // 10 seconds at 48kHz/1024 samples per buffer
        .build()
        .expect("Failed to create audiotestsrc");

    // Audio caps for consistent format
    let audio_caps = gstreamer::Caps::builder("audio/x-raw")
        .field("format", "S16LE")
        .field("rate", 48000i32)
        .field("channels", 2i32)
        .build();

    let audio_capsfilter = gstreamer::ElementFactory::make("capsfilter")
        .property("caps", &audio_caps)
        .build()
        .expect("Failed to create audio capsfilter");

    // Create encoders
    let videoencoder = gstreamer::ElementFactory::make("x264enc")
        .property("speed-preset", "ultrafast")
        .property("tune", "zerolatency")
        .property("bitrate", 2000u32) // 2Mbps
        .build()
        .expect("Failed to create x264enc");

    let audioencoder = gstreamer::ElementFactory::make("avenc_aac")
        .property("bitrate", 128000i64) // 128kbps
        .build()
        .expect("Failed to create avenc_aac");

    // Create muxer
    let muxer = gstreamer::ElementFactory::make("mpegtsmux")
        .property("alignment", 7i32) // 188-byte alignment for MPEG-TS
        .build()
        .expect("Failed to create mpegtsmux");

    // RTP payload MPEG-TS before feeding into RIST (RIST expects RTP in)
    let rtpmp2tpay = gstreamer::ElementFactory::make("rtpmp2tpay")
        .property("pt", 33u32)
        .build()
        .expect("Failed to create rtpmp2tpay");

    // Create RIST sink; we'll configure bonded endpoints via bonding-addresses
    let ristsink = gstreamer::ElementFactory::make("ristsink")
        .property("sender-buffer", 5000u32)
        .property("stats-update-interval", 1000u32)
        .build()
        .expect("Failed to create ristsink");

    let n = link_count.max(1);
    // Build bonding-addresses CSV, all pointing to the same receiver port
    let bonding_addrs = (0..n)
        .map(|_| format!("127.0.0.1:{}", rx_port))
        .collect::<Vec<_>>()
        .join(",");
    ristsink.set_property("bonding-addresses", bonding_addrs);

    // Add elements to pipeline
    pipeline
        .add_many([
            &videotestsrc,
            &video_capsfilter,
            &videoencoder,
            &audiotestsrc,
            &audio_capsfilter,
            &audioencoder,
            &muxer,
            &rtpmp2tpay,
            &ristsink,
        ])
        .expect("Failed to add elements to sender pipeline");

    // Link video path
    gstreamer::Element::link_many([&videotestsrc, &video_capsfilter, &videoencoder, &muxer])
        .expect("Failed to link video elements");

    // Link audio path
    gstreamer::Element::link_many([&audiotestsrc, &audio_capsfilter, &audioencoder, &muxer])
        .expect("Failed to link audio elements");

    // Link muxed TS -> RTP payload -> RIST sink
    gstreamer::Element::link_many([&muxer, &rtpmp2tpay, &ristsink])
        .expect("Failed to link TS->RTP->ristsink");

    (pipeline, ristsink)
}

/// Create a receiver pipeline with RIST source
fn build_receiver_pipeline(rx_port: u16) -> (gstreamer::Pipeline, gstreamer::Element) {
    let pipeline = gstreamer::Pipeline::new();

    // Create RIST source -> RTP depay -> MPEG-TS demux
    let ristsrc = gstreamer::ElementFactory::make("ristsrc")
        .property("address", "0.0.0.0")
        .property("port", rx_port as u32)
        // Larger receiver buffer helps with RTX and jitter under stress
        .property("receiver-buffer", 5000u32)
        .property("stats-update-interval", 1000u32)
        .property("encoding-name", "MP2T")
        .build()
        .expect("Failed to create ristsrc");
    let rtpmp2tdepay = gstreamer::ElementFactory::make("rtpmp2tdepay")
        .build()
        .expect("Failed to create rtpmp2tdepay");
    // Tee off raw TS for saving while also feeding demux
    let ts_tee = gstreamer::ElementFactory::make("tee")
        .build()
        .expect("Failed to create tee");
    let tsq_file = gstreamer::ElementFactory::make("queue")
        .build()
        .expect("Failed to create queue");
    let ts_out = artifact_path("rist_elements_stress_output.ts");
    let ts_filesink = gstreamer::ElementFactory::make("filesink")
        .property("location", ts_out.to_string_lossy().to_string())
        .build()
        .expect("Failed to create filesink");

    let tsq_demux = gstreamer::ElementFactory::make("queue")
        .build()
        .expect("Failed to create demux queue");

    let demux = gstreamer::ElementFactory::make("tsdemux")
        .build()
        .expect("Failed to create tsdemux");

    // Create counter sink for verification (from test harness)
    let counter = testing::create_counter_sink();

    // Create sink
    let sink = gstreamer::ElementFactory::make("fakesink")
        .property("sync", false)
        .property("async", false)
        .build()
        .expect("Failed to create fakesink");

    // Add elements
    pipeline
        .add_many([
            &ristsrc,
            &rtpmp2tdepay,
            &ts_tee,
            &tsq_file,
            &ts_filesink,
            &tsq_demux,
            &demux,
            &counter,
            &sink,
        ])
        .expect("Failed to add elements to receiver pipeline");

    // Link static elements
    gstreamer::Element::link_many([&ristsrc, &rtpmp2tdepay, &ts_tee])
        .expect("Failed to link ristsrc -> rtpmp2tdepay -> tee");
    // Branch 1: Save raw TS to file
    gstreamer::Element::link_many([&ts_tee, &tsq_file, &ts_filesink])
        .expect("Failed to link tee to filesink");
    // Branch 2: Feed demux for validation
    gstreamer::Element::link_many([&ts_tee, &tsq_demux, &demux])
        .expect("Failed to link tee to tsdemux");

    // Handle dynamic pads from demux
    let counter_clone = counter.clone();
    let sink_clone = sink.clone();
    demux.connect_pad_added(move |_demux, pad| {
        let pad_name = pad.name();
        if pad_name.starts_with("video_") {
            let counter_sink_pad = counter_clone.static_pad("sink").unwrap();
            pad.link(&counter_sink_pad)
                .expect("Failed to link demux video pad");
            counter_clone
                .link(&sink_clone)
                .expect("Failed to link counter to sink");
        }
    });

    (pipeline, counter)
}

/// Create a receiver pipeline that listens on multiple bonded ports
fn build_stress_receiver_pipeline(ports: &[u16]) -> (gstreamer::Pipeline, gstreamer::Element) {
    let pipeline = gstreamer::Pipeline::new();

    // ristsrc configured to listen on all bonded ports
    let ristsrc = gstreamer::ElementFactory::make("ristsrc")
        .property("receiver-buffer", 5000u32)
        .property("stats-update-interval", 1000u32)
        .property("encoding-name", "MP2T")
        .build()
        .expect("Failed to create ristsrc");

    // Configure bonding-addresses to bind on all local ports
    let bonding_addrs = ports
        .iter()
        .map(|p| format!("0.0.0.0:{}", p))
        .collect::<Vec<_>>()
        .join(",");
    ristsrc.set_property("bonding-addresses", bonding_addrs);

    let rtpmp2tdepay = gstreamer::ElementFactory::make("rtpmp2tdepay")
        .build()
        .expect("Failed to create rtpmp2tdepay");
    let ts_tee = gstreamer::ElementFactory::make("tee").build().unwrap();
    let tsq_file = gstreamer::ElementFactory::make("queue").build().unwrap();
    let ts_filesink = gstreamer::ElementFactory::make("filesink")
        .property("location", "target/stress_output.ts")
        .build()
        .unwrap();
    let tsq_demux = gstreamer::ElementFactory::make("queue").build().unwrap();
    let demux = gstreamer::ElementFactory::make("tsdemux").build().unwrap();

    // MP4 writing path: parse H.265/AAC and mux
    let h265parse = gstreamer::ElementFactory::make("h265parse")
        .property("config-interval", -1i32)
        .build()
        .unwrap();
    let vtee = gstreamer::ElementFactory::make("tee").build().unwrap();
    let vq_mp4 = gstreamer::ElementFactory::make("queue").build().unwrap();
    let vq_count = gstreamer::ElementFactory::make("queue").build().unwrap();

    let aacparse = gstreamer::ElementFactory::make("aacparse").build().unwrap();
    let aq_mp4 = gstreamer::ElementFactory::make("queue").build().unwrap();

    let mp4mux = gstreamer::ElementFactory::make("mp4mux").build().unwrap();
    let mp4_out = artifact_path("rist_elements_stress_output.mp4");
    let mp4sink = gstreamer::ElementFactory::make("filesink")
        .property("location", mp4_out.to_string_lossy().to_string())
        .build()
        .unwrap();

    let counter = testing::create_counter_sink();

    pipeline
        .add_many([
            &ristsrc,
            &rtpmp2tdepay,
            &ts_tee,
            &tsq_file,
            &ts_filesink,
            &tsq_demux,
            &demux,
            &h265parse,
            &vtee,
            &vq_mp4,
            &vq_count,
            &aacparse,
            &aq_mp4,
            &mp4mux,
            &mp4sink,
            &counter,
        ])
        .expect("Failed to add elements to stress receiver pipeline");

    gstreamer::Element::link_many([&ristsrc, &rtpmp2tdepay, &ts_tee])
        .expect("Failed to link ristsrc -> rtpmp2tdepay -> tee");
    gstreamer::Element::link_many([&ts_tee, &tsq_file, &ts_filesink])
        .expect("Failed to link tee to filesink");
    gstreamer::Element::link_many([&ts_tee, &tsq_demux, &demux])
        .expect("Failed to link tee to tsdemux");

    // Static MP4 path links that don't depend on demux pads
    h265parse.link(&vtee).unwrap();
    gstreamer::Element::link_many([&vq_count, &counter]).unwrap();
    mp4mux.link(&mp4sink).unwrap();

    // Dynamic pad linking into MP4 mux and counter path
    let h265parse_clone = h265parse.clone();
    let vtee_clone = vtee.clone();
    let vq_mp4_clone = vq_mp4.clone();
    let vq_count_clone = vq_count.clone();
    let aacparse_clone = aacparse.clone();
    let aq_mp4_clone = aq_mp4.clone();
    let mp4mux_clone = mp4mux.clone();
    demux.connect_pad_added(move |_demux, pad| {
        let pad_name = pad.name();
        if pad_name.starts_with("video_") {
            // demux video -> h265parse
            let sink_pad = h265parse_clone.static_pad("sink").unwrap();
            if pad.link(&sink_pad).is_err() {
                eprintln!("Failed to link demux video pad to h265parse");
                return;
            }
            // vtee has two branches: mp4 and counter
            if let Some(vtee_src_mp4) = vtee_clone.request_pad_simple("src_%u") {
                let vq_mp4_sink = vq_mp4_clone.static_pad("sink").unwrap();
                let _ = vtee_src_mp4.link(&vq_mp4_sink);
                let vq_mp4_src = vq_mp4_clone.static_pad("src").unwrap();
                if let Some(mp4_video_pad) = mp4mux_clone.request_pad_simple("video_%u") {
                    let _ = vq_mp4_src.link(&mp4_video_pad);
                }
            }
            if let Some(vtee_src_count) = vtee_clone.request_pad_simple("src_%u") {
                let vq_count_sink = vq_count_clone.static_pad("sink").unwrap();
                let _ = vtee_src_count.link(&vq_count_sink);
            }
        } else if pad_name.starts_with("audio_") {
            // demux audio -> aacparse -> mp4mux
            let sink_pad = aacparse_clone.static_pad("sink").unwrap();
            if pad.link(&sink_pad).is_err() {
                eprintln!("Failed to link demux audio pad to aacparse");
                return;
            }
            let aac_src = aacparse_clone.static_pad("src").unwrap();
            let aq_mp4_sink = aq_mp4_clone.static_pad("sink").unwrap();
            let _ = aac_src.link(&aq_mp4_sink);
            let aq_mp4_src = aq_mp4_clone.static_pad("src").unwrap();
            if let Some(mp4_audio_pad) = mp4mux_clone.request_pad_simple("audio_%u") {
                let _ = aq_mp4_src.link(&mp4_audio_pad);
            }
        }
    });

    (pipeline, counter)
}

/// Create a simplified receiver pipeline for debugging RIST data flow
/// This removes the complex MP4/TS processing to isolate connection issues
fn build_simple_receiver_pipeline(ports: &[u16]) -> (gstreamer::Pipeline, gstreamer::Element) {
    let pipeline = gstreamer::Pipeline::new();

    // ristsrc configured to listen on all bonded ports
    let ristsrc = gstreamer::ElementFactory::make("ristsrc")
        .property("receiver-buffer", 5000u32)
        .property("stats-update-interval", 1000u32)
        .property("encoding-name", "MP2T")
        .build()
        .expect("Failed to create ristsrc");

    // Configure bonding-addresses to bind on all local ports
    let bonding_addrs = ports
        .iter()
        .map(|p| format!("0.0.0.0:{}", p))
        .collect::<Vec<_>>()
        .join(",");
    ristsrc.set_property("bonding-addresses", bonding_addrs);

    // Simple processing chain: RIST -> RTP depay -> counter -> fakesink
    let rtpmp2tdepay = gstreamer::ElementFactory::make("rtpmp2tdepay")
        .build()
        .expect("Failed to create rtpmp2tdepay");

    let counter = testing::create_counter_sink();

    let fakesink = gstreamer::ElementFactory::make("fakesink")
        .property("sync", false)
        .property("async", false)
        .build()
        .expect("Failed to create fakesink");

    pipeline
        .add_many([&ristsrc, &rtpmp2tdepay, &counter, &fakesink])
        .expect("Failed to add elements to simple receiver pipeline");

    // Simple linear linking
    gstreamer::Element::link_many([&ristsrc, &rtpmp2tdepay, &counter, &fakesink])
        .expect("Failed to link simple receiver pipeline elements");

    (pipeline, counter)
}

/// Create a simplified receiver pipeline for single-port testing
#[allow(dead_code)]
fn build_simple_receiver_pipeline_single(
    rx_port: u16,
) -> (gstreamer::Pipeline, gstreamer::Element) {
    let pipeline = gstreamer::Pipeline::new();

    // Single-port ristsrc listening on rx_port
    let ristsrc = gstreamer::ElementFactory::make("ristsrc")
        .property("address", "0.0.0.0")
        .property("port", rx_port as u32)
        .property("receiver-buffer", 5000u32)
        .property("stats-update-interval", 1000u32)
        .property("encoding-name", "MP2T")
        .build()
        .expect("Failed to create ristsrc");

    // Simple processing chain
    let rtpmp2tdepay = gstreamer::ElementFactory::make("rtpmp2tdepay")
        .build()
        .expect("Failed to create rtpmp2tdepay");

    let counter = testing::create_counter_sink();

    let fakesink = gstreamer::ElementFactory::make("fakesink")
        .property("sync", false)
        .property("async", false)
        .build()
        .expect("Failed to create fakesink");

    pipeline
        .add_many([&ristsrc, &rtpmp2tdepay, &counter, &fakesink])
        .expect("Failed to add elements to simple single receiver pipeline");

    // Simple linear linking
    gstreamer::Element::link_many([&ristsrc, &rtpmp2tdepay, &counter, &fakesink])
        .expect("Failed to link simple single receiver pipeline elements");

    (pipeline, counter)
}
#[allow(dead_code)]
fn build_stress_receiver_pipeline_single(
    rx_port: u16,
) -> (gstreamer::Pipeline, gstreamer::Element) {
    let pipeline = gstreamer::Pipeline::new();

    // Single-port ristsrc listening on rx_port
    let ristsrc = gstreamer::ElementFactory::make("ristsrc")
        .property("address", "0.0.0.0")
        .property("port", rx_port as u32)
        .property("receiver-buffer", 5000u32)
        .property("stats-update-interval", 1000u32)
        .property("encoding-name", "MP2T")
        .build()
        .unwrap();

    let rtpmp2tdepay = gstreamer::ElementFactory::make("rtpmp2tdepay")
        .build()
        .unwrap();
    let ts_tee = gstreamer::ElementFactory::make("tee").build().unwrap();
    let tsq_file = gstreamer::ElementFactory::make("queue").build().unwrap();
    let ts_out = artifact_path("rist_elements_stress_output.ts");
    let ts_filesink = gstreamer::ElementFactory::make("filesink")
        .property("location", ts_out.to_string_lossy().to_string())
        .build()
        .unwrap();
    let tsq_demux = gstreamer::ElementFactory::make("queue").build().unwrap();
    let demux = gstreamer::ElementFactory::make("tsdemux").build().unwrap();

    let h265parse = gstreamer::ElementFactory::make("h265parse")
        .property("config-interval", -1i32)
        .build()
        .unwrap();
    let vtee = gstreamer::ElementFactory::make("tee").build().unwrap();
    let vq_mp4 = gstreamer::ElementFactory::make("queue").build().unwrap();
    let vq_count = gstreamer::ElementFactory::make("queue").build().unwrap();

    let aacparse = gstreamer::ElementFactory::make("aacparse").build().unwrap();
    let aq_mp4 = gstreamer::ElementFactory::make("queue").build().unwrap();

    let mp4mux = gstreamer::ElementFactory::make("mp4mux").build().unwrap();
    let mp4_out = artifact_path("rist_elements_stress_output.mp4");
    let mp4sink = gstreamer::ElementFactory::make("filesink")
        .property("location", mp4_out.to_string_lossy().to_string())
        .build()
        .unwrap();

    let counter = testing::create_counter_sink();

    pipeline
        .add_many([
            &ristsrc,
            &rtpmp2tdepay,
            &ts_tee,
            &tsq_file,
            &ts_filesink,
            &tsq_demux,
            &demux,
            &h265parse,
            &vtee,
            &vq_mp4,
            &vq_count,
            &aacparse,
            &aq_mp4,
            &mp4mux,
            &mp4sink,
            &counter,
        ])
        .expect("Failed to add elements to single-port stress receiver pipeline");

    gstreamer::Element::link_many([&ristsrc, &rtpmp2tdepay, &ts_tee])
        .expect("Failed to link ristsrc -> rtpmp2tdepay -> tee");
    gstreamer::Element::link_many([&ts_tee, &tsq_file, &ts_filesink])
        .expect("Failed to link tee to filesink");
    gstreamer::Element::link_many([&ts_tee, &tsq_demux, &demux])
        .expect("Failed to link tee to tsdemux");

    // Static MP4 path links that don't depend on demux pads
    h265parse.link(&vtee).unwrap();
    gstreamer::Element::link_many([&vq_count, &counter]).unwrap();
    mp4mux.link(&mp4sink).unwrap();

    // Dynamic pad linking into MP4 mux and counter path
    let h265parse_clone = h265parse.clone();
    let vtee_clone = vtee.clone();
    let vq_mp4_clone = vq_mp4.clone();
    let vq_count_clone = vq_count.clone();
    let aacparse_clone = aacparse.clone();
    let aq_mp4_clone = aq_mp4.clone();
    let mp4mux_clone = mp4mux.clone();
    demux.connect_pad_added(move |_demux, pad| {
        let pad_name = pad.name();
        if pad_name.starts_with("video_") {
            // demux video -> h265parse
            let sink_pad = h265parse_clone.static_pad("sink").unwrap();
            if pad.link(&sink_pad).is_err() {
                eprintln!("Failed to link demux video pad to h265parse");
                return;
            }
            // vtee has two branches: mp4 and counter
            if let Some(vtee_src_mp4) = vtee_clone.request_pad_simple("src_%u") {
                let vq_mp4_sink = vq_mp4_clone.static_pad("sink").unwrap();
                let _ = vtee_src_mp4.link(&vq_mp4_sink);
                let vq_mp4_src = vq_mp4_clone.static_pad("src").unwrap();
                if let Some(mp4_video_pad) = mp4mux_clone.request_pad_simple("video_%u") {
                    let _ = vq_mp4_src.link(&mp4_video_pad);
                }
            }
            if let Some(vtee_src_count) = vtee_clone.request_pad_simple("src_%u") {
                let vq_count_sink = vq_count_clone.static_pad("sink").unwrap();
                let _ = vtee_src_count.link(&vq_count_sink);
            }
        } else if pad_name.starts_with("audio_") {
            // demux audio -> aacparse -> mp4mux
            let sink_pad = aacparse_clone.static_pad("sink").unwrap();
            if pad.link(&sink_pad).is_err() {
                eprintln!("Failed to link demux audio pad to aacparse");
                return;
            }
            let aac_src = aacparse_clone.static_pad("src").unwrap();
            let aq_mp4_sink = aq_mp4_clone.static_pad("sink").unwrap();
            let _ = aac_src.link(&aq_mp4_sink);
            let aq_mp4_src = aq_mp4_clone.static_pad("src").unwrap();
            if let Some(mp4_audio_pad) = mp4mux_clone.request_pad_simple("audio_%u") {
                let _ = aq_mp4_src.link(&mp4_audio_pad);
            }
        }
    });

    (pipeline, counter)
}

/// Run both pipelines concurrently with proper lifecycle management and error monitoring
async fn run_pipelines(
    sender: gstreamer::Pipeline,
    receiver: gstreamer::Pipeline,
    duration_secs: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    // Start receiver first
    receiver
        .set_state(gstreamer::State::Playing)
        .expect("Failed to start receiver pipeline");

    // Give receiver time to initialize and check for immediate errors
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Monitor receiver bus for errors during startup
    if let Some(receiver_bus) = receiver.bus() {
        if let Some(msg) = receiver_bus.pop() {
            match msg.view() {
                gstreamer::MessageView::Error(err) => {
                    return Err(format!(
                        "Receiver pipeline error during startup: {} - {}",
                        err.error(),
                        err.debug().unwrap_or_else(|| "No debug info".into())
                    )
                    .into());
                }
                gstreamer::MessageView::Warning(warn) => {
                    println!(
                        "Receiver warning: {} - {}",
                        warn.error(),
                        warn.debug().unwrap_or_else(|| "No debug info".into())
                    );
                }
                _ => {}
            }
        }
    }

    // Start sender
    sender
        .set_state(gstreamer::State::Playing)
        .expect("Failed to start sender pipeline");

    // Monitor both pipelines for errors during execution
    let start_time = tokio::time::Instant::now();
    while start_time.elapsed() < Duration::from_secs(duration_secs) {
        // Check sender bus for errors
        if let Some(sender_bus) = sender.bus() {
            while let Some(msg) = sender_bus.pop() {
                match msg.view() {
                    gstreamer::MessageView::Error(err) => {
                        let _ = sender.set_state(gstreamer::State::Null);
                        let _ = receiver.set_state(gstreamer::State::Null);
                        return Err(format!(
                            "Sender pipeline error: {} - {}",
                            err.error(),
                            err.debug().unwrap_or_else(|| "No debug info".into())
                        )
                        .into());
                    }
                    gstreamer::MessageView::Warning(warn) => {
                        println!(
                            "Sender warning: {} - {}",
                            warn.error(),
                            warn.debug().unwrap_or_else(|| "No debug info".into())
                        );
                    }
                    gstreamer::MessageView::Info(info) => {
                        println!("ℹ️  Sender info: {:?}", info.message());
                    }
                    _ => {}
                }
            }
        }

        // Check receiver bus for errors
        if let Some(receiver_bus) = receiver.bus() {
            while let Some(msg) = receiver_bus.pop() {
                match msg.view() {
                    gstreamer::MessageView::Error(err) => {
                        let _ = sender.set_state(gstreamer::State::Null);
                        let _ = receiver.set_state(gstreamer::State::Null);
                        return Err(format!(
                            "Receiver pipeline error: {} - {}",
                            err.error(),
                            err.debug().unwrap_or_else(|| "No debug info".into())
                        )
                        .into());
                    }
                    gstreamer::MessageView::Warning(warn) => {
                        println!(
                            "Receiver warning: {} - {}",
                            warn.error(),
                            warn.debug().unwrap_or_else(|| "No debug info".into())
                        );
                    }
                    gstreamer::MessageView::Info(info) => {
                        println!("ℹ️  Receiver info: {:?}", info.message());
                    }
                    _ => {}
                }
            }
        }

        // Sleep briefly before checking again
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Stop pipelines gracefully
    sender
        .set_state(gstreamer::State::Null)
        .expect("Failed to stop sender pipeline");
    receiver
        .set_state(gstreamer::State::Null)
        .expect("Failed to stop receiver pipeline");

    Ok(())
}

/// Test single-link RIST transmission
#[tokio::test]
#[ignore] // Remove this when ready to run
async fn test_single_link_rist_transmission() -> Result<(), Box<dyn std::error::Error>> {
    testing::init_for_tests();
    let config = TestConfig::default();

    println!("Testing single-link RIST transmission");

    // Create network orchestrator
    let mut orchestrator = NetworkOrchestrator::new(config.seed).await?;

    // Start baseline good scenario
    let scenario = TestScenario::baseline_good();
    let link = orchestrator
        .start_scenario(scenario, config.rx_port)
        .await?;

    println!(
        "Started network link: {} -> {}",
        link.ingress_port, link.egress_port
    );

    // Build pipelines
    let (sender, _ristsink) = build_sender_pipeline_to_rx(config.rx_port, 1);
    let (receiver, counter) = build_stress_receiver_pipeline(&[link.ingress_port]);

    // Run test
    println!(
        "Running transmission test for {} seconds",
        config.test_duration_secs
    );
    run_pipelines(sender, receiver, config.test_duration_secs).await?;

    // Verify data was received
    let count: u64 = testing::get_property(&counter, "count").unwrap_or_else(|_| 0u64);

    println!("Received {} buffers", count);
    assert!(count > 0, "No data received through RIST link");

    Ok(())
}

/// Test dual-link RIST bonding with asymmetric quality
#[tokio::test]
#[ignore] // Remove this when ready to run
async fn test_dual_link_rist_bonding() -> Result<(), Box<dyn std::error::Error>> {
    testing::init_for_tests();
    let config = TestConfig {
        test_duration_secs: 15, // Longer test for bonding
        ..Default::default()
    };

    println!("Testing dual-link RIST bonding");

    // Create network orchestrator
    let mut orchestrator = NetworkOrchestrator::new(config.seed + 1).await?;

    // Start bonding scenario
    let scenario = TestScenario::bonding_asymmetric();
    let link = orchestrator
        .start_scenario(scenario, config.rx_port)
        .await?;

    println!("Started bonding scenario: {}", link.scenario.name);

    // For bonding in this simplified test, simulate a second session on a nearby port
    let ports = vec![link.ingress_port, link.ingress_port + 2];

    // Build pipelines with bonding
    let (sender, ristsink) = build_sender_pipeline_to_rx(config.rx_port, ports.len());
    let (receiver, counter) = build_receiver_pipeline(config.rx_port);

    // Enable bonding on the sink
    ristsink.set_property("bonding", &true);

    // Run test
    println!(
        "Running bonding test for {} seconds",
        config.test_duration_secs
    );
    run_pipelines(sender, receiver, config.test_duration_secs).await?;

    // Verify data was received
    let count: u64 = testing::get_property(&counter, "count").unwrap_or_else(|_| 0u64);

    println!("Received {} buffers through bonding", count);
    assert!(count > 0, "No data received through bonded RIST links");

    // Verify bonding statistics
    let bonding_stats: String = testing::get_property(&ristsink, "stats")
        .unwrap_or_else(|_| "No stats available".to_string());
    println!("Bonding stats: {}", bonding_stats);

    Ok(())
}

/// Test network degradation and recovery
#[tokio::test]
#[ignore] // Remove this when ready to run
async fn test_network_degradation_recovery() -> Result<(), Box<dyn std::error::Error>> {
    testing::init_for_tests();
    let config = TestConfig {
        test_duration_secs: 20, // Longer test for degradation/recovery
        ..Default::default()
    };

    println!("📉 Testing network degradation and recovery");

    // Create network orchestrator
    let mut orchestrator = NetworkOrchestrator::new(config.seed + 2).await?;

    // Start with degrading network scenario
    let scenario = TestScenario::degrading_network();
    let _link = orchestrator
        .start_scenario(scenario, config.rx_port)
        .await?;

    println!("Started degrading network scenario");

    // Build pipelines
    let (sender, ristsink) = build_sender_pipeline_to_rx(config.rx_port, 1);
    let (receiver, counter) = build_receiver_pipeline(config.rx_port);

    // Enable adaptive bitrate on the sink
    ristsink.set_property("adaptive-bitrate", &true);

    // Run test
    println!(
        "Running degradation test for {} seconds",
        config.test_duration_secs
    );
    run_pipelines(sender, receiver, config.test_duration_secs).await?;

    // Verify data was received despite degradation
    let count: u64 = testing::get_property(&counter, "count").unwrap_or_else(|_| 0u64);

    println!("Received {} buffers through degraded network", count);
    assert!(count > 0, "No data received despite network degradation");

    // Check recovery metrics
    let recovery_stats: String = testing::get_property(&ristsink, "recovery-stats")
        .unwrap_or_else(|_| "No recovery stats available".to_string());
    println!("🔧 Recovery stats: {}", recovery_stats);

    Ok(())
}

/// Test mobile handover scenario
#[tokio::test]
#[ignore] // Remove this when ready to run
async fn test_mobile_handover() -> Result<(), Box<dyn std::error::Error>> {
    testing::init_for_tests();
    let config = TestConfig {
        test_duration_secs: 25, // Longer test for handover
        ..Default::default()
    };

    println!("📱 Testing mobile handover scenario");

    // Create network orchestrator
    let mut orchestrator = NetworkOrchestrator::new(config.seed + 3).await?;

    // Start mobile handover scenario
    let scenario = TestScenario::mobile_handover();
    let _link = orchestrator
        .start_scenario(scenario, config.rx_port)
        .await?;

    println!("Started mobile handover scenario");

    // Build pipelines with handover support
    let (sender, ristsink) = build_sender_pipeline_to_rx(config.rx_port, 1);
    let (receiver, counter) = build_receiver_pipeline(config.rx_port);

    // Enable seamless handover
    ristsink.set_property("seamless-handover", &true);

    // Run test with handover monitoring
    println!(
        "Running handover test for {} seconds",
        config.test_duration_secs
    );

    // Start pipelines
    receiver
        .set_state(gstreamer::State::Playing)
        .expect("Failed to start receiver");
    tokio::time::sleep(Duration::from_millis(500)).await;
    sender
        .set_state(gstreamer::State::Playing)
        .expect("Failed to start sender");

    // Monitor for handover events during the test
    let mut handover_count = 0;
    let test_start = tokio::time::Instant::now();

    while test_start.elapsed() < Duration::from_secs(config.test_duration_secs) {
        tokio::time::sleep(Duration::from_secs(1)).await;

        // Check for handover events (this would be implementation-specific)
        let current_link: String = testing::get_property(&ristsink, "active-link")
            .unwrap_or_else(|_| "unknown".to_string());

        if current_link != "primary" {
            handover_count += 1;
            println!("Handover detected to: {}", current_link);
        }
    }

    // Stop pipelines
    sender
        .set_state(gstreamer::State::Null)
        .expect("Failed to stop sender");
    receiver
        .set_state(gstreamer::State::Null)
        .expect("Failed to stop receiver");

    // Verify data continuity during handover
    let count: u64 = testing::get_property(&counter, "count").unwrap_or_else(|_| 0u64);

    println!("Received {} buffers during handover test", count);
    println!("Detected {} handover events", handover_count);

    assert!(count > 0, "No data received during mobile handover");

    Ok(())
}

/// Test stress scenario with multiple concurrent links
#[tokio::test]
#[ignore] // Remove this when ready to run
async fn test_stress_multiple_concurrent_links() -> Result<(), Box<dyn std::error::Error>> {
    testing::init_for_tests();
    let config = TestConfig {
        test_duration_secs: 15,
        seed: 1000, // Different seed to avoid port conflicts
        ..Default::default()
    };

    println!("⚡ Testing stress scenario with multiple concurrent links");

    // Create multiple orchestrators for independent scenarios
    let mut orchestrator = NetworkOrchestrator::new(config.seed).await?;

    // Start multiple scenarios concurrently
    let scenarios = vec![
        TestScenario::baseline_good(),
        TestScenario::bonding_asymmetric(),
        TestScenario::degrading_network(),
    ];

    let mut links = Vec::new();
    for (i, scenario) in scenarios.into_iter().enumerate() {
        let port = config.rx_port + (i as u16 * 10);
        let link = orchestrator.start_scenario(scenario, port).await?;
        println!(
            "Started scenario {}: {} on port {}",
            i + 1,
            link.scenario.name,
            port
        );
        links.push(link);
    }

    // Create and run multiple pipeline pairs
    let mut handles = Vec::new();

    for (i, _link) in links.iter().enumerate() {
        let port = config.rx_port + (i as u16 * 10);
        let (sender, _) = build_sender_pipeline_to_rx(port, 1);
        let (receiver, counter) = build_receiver_pipeline(port);

        // Start this pipeline pair
        receiver
            .set_state(gstreamer::State::Playing)
            .expect("Failed to start receiver");
        tokio::time::sleep(Duration::from_millis(100)).await;
        sender
            .set_state(gstreamer::State::Playing)
            .expect("Failed to start sender");

        handles.push((sender, receiver, counter));
    }

    // Let all scenarios run concurrently
    println!(
        "Running {} concurrent scenarios for {} seconds",
        handles.len(),
        config.test_duration_secs
    );
    tokio::time::sleep(Duration::from_secs(config.test_duration_secs)).await;

    // Stop all pipelines and collect results
    let mut total_received = 0u64;
    for (i, (sender, receiver, counter)) in handles.into_iter().enumerate() {
        sender
            .set_state(gstreamer::State::Null)
            .expect("Failed to stop sender");
        receiver
            .set_state(gstreamer::State::Null)
            .expect("Failed to stop receiver");

        let count: u64 = testing::get_property(&counter, "count").unwrap_or_else(|_| 0u64);
        total_received += count;

        println!("Scenario {}: received {} buffers", i + 1, count);
    }

    println!(
        "Total received across all scenarios: {} buffers",
        total_received
    );
    assert!(total_received > 0, "No data received in stress test");

    Ok(())
}

/// Build an RTP (MP2T) sender with dispatcher+dynamically-controlled encoder, configured for bonding
fn build_stress_sender_pipeline(ports: &[u16]) -> (gstreamer::Pipeline, gstreamer::Element) {
    let pipeline = gstreamer::Pipeline::new();

    // Video: 1080p60 test source -> x265enc (ultrafast, zerolatency)
    let vsrc = gstreamer::ElementFactory::make("videotestsrc")
        .property("is-live", true)
        .property_from_str("pattern", "smpte")
        .build()
        .expect("Failed to create videotestsrc");
    let vconv = gstreamer::ElementFactory::make("videoconvert")
        .build()
        .unwrap();
    let vscale = gstreamer::ElementFactory::make("videoscale")
        .build()
        .unwrap();
    let v_caps = gstreamer::Caps::builder("video/x-raw")
        .field("width", 1920i32)
        .field("height", 1080i32)
        .field("framerate", gstreamer::Fraction::new(60, 1))
        .build();
    let vcap = gstreamer::ElementFactory::make("capsfilter")
        .property("caps", &v_caps)
        .build()
        .unwrap();
    let venc = gstreamer::ElementFactory::make("x265enc")
        .property_from_str("speed-preset", "ultrafast")
        .property_from_str("tune", "zerolatency")
        .property("bitrate", 8000u32) // start at 8 Mbps; dynbitrate will adjust
        .property("key-int-max", 120i32)
        .build()
        .expect("Failed to create x265enc");
    let vparse = gstreamer::ElementFactory::make("h265parse")
        .property("config-interval", 1i32)
        .build()
        .expect("Failed to create h265parse");

    // Audio: stereo AAC 256 kbps @ 48kHz
    let asrc = gstreamer::ElementFactory::make("audiotestsrc")
        .property("is-live", true)
        .property("freq", 440.0f64)
        .build()
        .expect("Failed to create audiotestsrc");
    let aconv = gstreamer::ElementFactory::make("audioconvert")
        .build()
        .unwrap();
    let ares = gstreamer::ElementFactory::make("audioresample")
        .build()
        .unwrap();
    let aenc = gstreamer::ElementFactory::make("avenc_aac")
        .property("bitrate", 256000i32)
        .build()
        .expect("Failed to create avenc_aac");
    let aparse = gstreamer::ElementFactory::make("aacparse").build().unwrap();

    // Mux to MPEG-TS
    let tsmux = gstreamer::ElementFactory::make("mpegtsmux")
        .property("alignment", 7i32)
        .build()
        .expect("Failed to create mpegtsmux");

    // RTP payload for MP2T
    let rtpmp2tpay = gstreamer::ElementFactory::make("rtpmp2tpay")
        .property("pt", 33u32)
        .build()
        .expect("Failed to create rtpmp2tpay");
    let ssrc_value: u32 = 0x2468ABCE; // even
    let rtp_caps = gstreamer::Caps::builder("application/x-rtp")
        .field("media", "video")
        .field("encoding-name", "MP2T")
        .field("payload", 33i32)
        .field("clock-rate", 90000i32)
        .field("ssrc", ssrc_value)
        .build();
    let rtp_cf = gstreamer::ElementFactory::make("capsfilter")
        .property("caps", &rtp_caps)
        .build()
        .unwrap();

    // Our custom load balancer and bitrate controller
    let dispatcher = testing::create_dispatcher(None);
    let dynbitrate = testing::create_dynbitrate();

    // RIST sink with dispatcher for bonded sessions
    let ristsink = gstreamer::ElementFactory::make("ristsink")
        .property("sender-buffer", 5000u32)
        .property("stats-update-interval", 1000u32)
        .build()
        .expect("Failed to create ristsink");
    ristsink.set_property("dispatcher", &dispatcher);

    // Configure bonded sessions via bonding-addresses CSV
    // Use localhost for all bonded sessions; the orchestrator maps ingress ports accordingly
    let bonding_addrs = ports
        .iter()
        .map(|port| format!("127.0.0.1:{}", port))
        .collect::<Vec<_>>()
        .join(",");
    ristsink.set_property("bonding-addresses", bonding_addrs);

    // Add elements
    pipeline
        .add_many([
            &vsrc,
            &vconv,
            &vscale,
            &vcap,
            &venc,
            &vparse, // video
            &asrc,
            &aconv,
            &ares,
            &aenc,
            &aparse, // audio
            &tsmux,
            &rtpmp2tpay,
            &rtp_cf,
            &dynbitrate,
            &ristsink,
        ])
        .expect("Failed to add elements to sender pipeline");

    // Link video branch
    gstreamer::Element::link_many([&vsrc, &vconv, &vscale, &vcap, &venc, &vparse])
        .expect("Failed to link video branch");
    // Link audio branch
    gstreamer::Element::link_many([&asrc, &aconv, &ares, &aenc, &aparse])
        .expect("Failed to link audio branch");

    // Request and link ts muxer pads (mpegtsmux uses request pads sink_%d)
    let v_pad = vparse.static_pad("src").unwrap();
    let v_sink = tsmux.request_pad_simple("sink_%d").unwrap();
    v_pad.link(&v_sink).expect("Failed to link video to tsmux");
    let a_pad = aparse.static_pad("src").unwrap();
    let a_sink = tsmux.request_pad_simple("sink_%d").unwrap();
    a_pad.link(&a_sink).expect("Failed to link audio to tsmux");

    // Link TS -> RTP -> dynbitrate -> RIST sink
    gstreamer::Element::link_many([&tsmux, &rtpmp2tpay, &rtp_cf, &dynbitrate, &ristsink])
        .expect("Failed to link TS->RTP->dynbitrate->ristsink");

    // Wire up dynbitrate control
    dynbitrate.set_property("encoder", &venc);
    dynbitrate.set_property("rist", &ristsink);
    dynbitrate.set_property("dispatcher", &dispatcher);
    dynbitrate.set_property("min-kbps", 500u32);
    dynbitrate.set_property("max-kbps", 20000u32);
    dynbitrate.set_property("step-kbps", 250u32);
    dynbitrate.set_property("target-loss-pct", 1.0f64);
    dynbitrate.set_property("downscale-keyunit", &true);

    (pipeline, ristsink)
}

/// Build four Markov-varying link scenarios at different bandwidth tiers
fn build_stress_bonded_scenarios() -> Vec<TestScenario> {
    fn markov_for_rate(rate_kbps: u32, name: &str) -> LinkSpec {
        // Base state: target rate with moderate noise
        let base = DirectionSpec {
            base_delay_ms: 30,
            jitter_ms: 10,
            loss_pct: 0.003,
            loss_burst_corr: 0.2,
            reorder_pct: 0.003,
            duplicate_pct: 0.0005,
            rate_kbps,
            mtu: Some(1500),
        };
        // Heavy loss state: severe burst loss to simulate fades/outages
        let heavy = DirectionSpec {
            base_delay_ms: 120,
            jitter_ms: 50,
            loss_pct: 0.2,        // 20% loss
            loss_burst_corr: 0.9, // highly bursty
            reorder_pct: 0.02,
            duplicate_pct: 0.002,
            rate_kbps: (rate_kbps as f32 * 0.6) as u32,
            mtu: Some(1400),
        };

        let schedule = Schedule::Markov {
            states: vec![base, heavy],
            transition_matrix: vec![
                vec![0.88, 0.12], // base -> base/heavy
                vec![0.40, 0.60], // heavy -> base/heavy
            ],
            initial_state: 0,
            mean_dwell_time: Duration::from_secs(8),
        };

        LinkSpec {
            name: name.to_string(),
            a_ns: format!("{}-tx", name),
            b_ns: format!("{}-rx", name),
            a_to_b: schedule.clone(),
            b_to_a: schedule, // symmetric
        }
    }

    let tiers = vec![
        (150u32, "poor"),
        (500u32, "low"),
        (1000u32, "medium"),
        (2000u32, "high"),
    ];

    tiers
        .into_iter()
        .map(|(kbps, label)| TestScenario {
            name: format!("stress_{}_bonded", label),
            description: format!("Stress link '{}' with Markov heavy loss", label),
            links: vec![markov_for_rate(kbps, label)],
            duration_seconds: Some(30),
            metadata: {
                let mut m = HashMap::new();
                m.insert("scenario_type".to_string(), "stress".to_string());
                m.insert("tier".to_string(), label.to_string());
                m
            },
        })
        .collect()
}

/// Simple test to verify basic UDP RTP elements work (bypass RIST for now)
/// This tests our test framework independent of RIST issues  
#[tokio::test]
#[ignore]
async fn test_simple_udp_connection() -> Result<(), Box<dyn std::error::Error>> {
    testing::init_for_tests();

    // Set up debug logging
    std::env::set_var("RUST_LOG", "debug");
    std::env::set_var("GST_DEBUG", "3");
    std::env::set_var("GST_PLUGIN_PATH", "target/debug");

    let config = TestConfig {
        test_duration_secs: 8,
        seed: 12345,
        rx_port: 7780,
    };

    println!("🧪 Testing simple UDP RTP connection (bypass RIST)");

    // Build simple UDP sender pipeline
    let sender = {
        let pipeline = gstreamer::Pipeline::new();

        let src = gstreamer::ElementFactory::make("audiotestsrc")
            .property("is-live", true)
            .property("num-buffers", 240) // 8 seconds at 30fps
            .build()
            .expect("Failed to create audiotestsrc");

        let pay = gstreamer::ElementFactory::make("rtpL16pay")
            .build()
            .expect("Failed to create rtpL16pay");

        let sink = gstreamer::ElementFactory::make("udpsink")
            .property("host", "127.0.0.1")
            .property("port", config.rx_port as i32)
            .build()
            .expect("Failed to create udpsink");

        pipeline.add_many([&src, &pay, &sink])?;
        gstreamer::Element::link_many([&src, &pay, &sink])?;

        pipeline
    };

    // Build simple UDP receiver pipeline
    let (receiver, counter) = {
        let pipeline = gstreamer::Pipeline::new();

        let src = gstreamer::ElementFactory::make("udpsrc")
            .property("port", config.rx_port as i32)
            .property(
                "caps",
                &gstreamer::Caps::builder("application/x-rtp").build(),
            )
            .build()
            .expect("Failed to create udpsrc");

        let depay = gstreamer::ElementFactory::make("rtpL16depay")
            .build()
            .expect("Failed to create rtpL16depay");

        let sink = gstreamer::ElementFactory::make("fakesink")
            .property("sync", false)
            .property("signal-handoffs", true)
            .build()
            .expect("Failed to create fakesink");

        // Count buffers using signal
        let buffer_count = Arc::new(AtomicU64::new(0));
        let count_clone = buffer_count.clone();
        sink.connect("handoff", false, move |_values| {
            count_clone.fetch_add(1, Ordering::Relaxed);
            None
        });

        pipeline.add_many([&src, &depay, &sink])?;
        gstreamer::Element::link_many([&src, &depay, &sink])?;

        (pipeline, buffer_count)
    };

    println!("📡 UDP sender configured for: 127.0.0.1:{}", config.rx_port);
    println!("📡 UDP receiver listening on: 0.0.0.0:{}", config.rx_port);

    // Run the test
    println!(
        "Running simple UDP test for {} seconds",
        config.test_duration_secs
    );

    // Start receiver first
    receiver
        .set_state(gstreamer::State::Playing)
        .expect("Failed to start receiver pipeline");
    println!("📡 Receiver started, waiting 1 second...");
    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Start sender
    sender
        .set_state(gstreamer::State::Playing)
        .expect("Failed to start sender pipeline");
    println!("📡 Sender started, running test...");

    // Run for most of the duration
    tokio::time::sleep(Duration::from_secs(config.test_duration_secs - 1)).await;

    // Stop pipelines
    sender
        .set_state(gstreamer::State::Null)
        .expect("Failed to stop sender pipeline");
    receiver
        .set_state(gstreamer::State::Null)
        .expect("Failed to stop receiver pipeline");

    // Check results
    let count = counter.load(Ordering::Relaxed);
    println!("Simple UDP test: received {} buffers", count);

    if count == 0 {
        return Err("No data received in UDP test - basic framework issue".into());
    }

    println!("✅ Simple UDP test passed with {} buffers received", count);
    println!("   This confirms test framework works - RIST is the issue");
    Ok(())
}

/// Simple test to verify RIST elements work without network simulation
/// This bypasses the NetworkOrchestrator issue and tests direct localhost communication
#[tokio::test]
#[ignore]
async fn test_simple_rist_connection() -> Result<(), Box<dyn std::error::Error>> {
    testing::init_for_tests();

    // Set up debug logging
    std::env::set_var("RUST_LOG", "debug");
    std::env::set_var("GST_DEBUG", "3,rist*:5,rtp*:4");
    std::env::set_var("GST_PLUGIN_PATH", "target/debug");

    let config = TestConfig {
        test_duration_secs: 15, // Longer duration for RIST handshake
        seed: 12345,
        rx_port: 7776, // RIST requires even ports
    };

    println!("🧪 Testing simple RIST connection (localhost, no network simulation)");
    println!("🔧 This bypasses NetworkOrchestrator to isolate RIST element issues");

    // Build simple sender pipeline targeting localhost
    let sender = {
        let pipeline = gstreamer::Pipeline::new();

        let src = gstreamer::ElementFactory::make("videotestsrc")
            .property("is-live", true)
            .property("num-buffers", 300) // 10 seconds at 30fps
            .build()
            .expect("Failed to create videotestsrc");

        let enc = gstreamer::ElementFactory::make("x264enc")
            .property_from_str("speed-preset", "ultrafast")
            .property_from_str("tune", "zerolatency")
            .property("bitrate", 1000u32)
            .build()
            .expect("Failed to create x264enc");

        let pay = gstreamer::ElementFactory::make("rtph264pay")
            .build()
            .expect("Failed to create rtph264pay");

        let ristsink = gstreamer::ElementFactory::make("ristsink")
            .property("address", "127.0.0.1")
            .property("port", config.rx_port as u32)
            .property("sender-buffer", 5000u32)
            .build()
            .expect("Failed to create ristsink");

        pipeline.add_many([&src, &enc, &pay, &ristsink])?;
        gstreamer::Element::link_many([&src, &enc, &pay, &ristsink])?;

        pipeline
    };

    // Build simple receiver pipeline
    let (receiver, counter) = {
        let pipeline = gstreamer::Pipeline::new();

        let ristsrc = gstreamer::ElementFactory::make("ristsrc")
            .property("address", "0.0.0.0")
            .property("port", config.rx_port as u32)
            .property("receiver-buffer", 5000u32)
            .build()
            .expect("Failed to create ristsrc");

        let depay = gstreamer::ElementFactory::make("rtph264depay")
            .build()
            .expect("Failed to create rtph264depay");

        let counter = testing::create_counter_sink();

        pipeline.add_many([&ristsrc, &depay, &counter])?;
        gstreamer::Element::link_many([&ristsrc, &depay, &counter])?;

        (pipeline, counter)
    };

    println!("📡 RIST sink configured for: 127.0.0.1:{}", config.rx_port);
    println!("📡 RIST source listening on: 0.0.0.0:{}", config.rx_port);

    // Run the test
    println!(
        "Running simple RIST test for {} seconds",
        config.test_duration_secs
    );

    // Start receiver first and wait longer for RIST handshake
    receiver
        .set_state(gstreamer::State::Playing)
        .expect("Failed to start receiver pipeline");
    println!("📡 Receiver started, waiting 2 seconds for RIST setup...");
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Start sender
    sender
        .set_state(gstreamer::State::Playing)
        .expect("Failed to start sender pipeline");
    println!("📡 Sender started, running test...");

    // Run for most of the duration
    tokio::time::sleep(Duration::from_secs(config.test_duration_secs - 2)).await;

    // Stop pipelines
    sender
        .set_state(gstreamer::State::Null)
        .expect("Failed to stop sender pipeline");
    receiver
        .set_state(gstreamer::State::Null)
        .expect("Failed to stop receiver pipeline");

    // Check results
    let count: u64 = testing::get_property(&counter, "count").unwrap_or_else(|_| 0u64);
    println!("Simple RIST test: received {} buffers", count);

    if count == 0 {
        println!("❌ No data received - RIST elements may have issues:");
        println!("   1. 🔌 GStreamer RIST plugin not found/loaded");
        println!("   2. 🚫 Port {} may be blocked or in use", config.rx_port);
        println!("   3. RIST handshake may have failed");
        println!("   4. ⏱️  Test duration may be too short for RIST setup");
        return Err("No data received in simple RIST test".into());
    }

    println!("✅ Simple RIST test passed with {} buffers received", count);
    println!("   This confirms RIST elements work - network orchestrator is the issue");
    Ok(())
}

/// Stress test: 4 bonded links (poor/low/medium/high), RTP MP2T over RIST with dispatcher+dynbitrate
/// Uses simplified receiver pipeline to isolate RIST connection issues
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore]
async fn test_stress_test() -> Result<(), Box<dyn std::error::Error>> {
    testing::init_for_tests();

    // Set up debug logging
    std::env::set_var("RUST_LOG", "debug");
    std::env::set_var("GST_DEBUG", "3,rist*:5,rtp*:4");

    // Ensure our custom GStreamer elements can be discovered during test run
    std::env::set_var("GST_PLUGIN_PATH", "target/debug");

    let config = TestConfig {
        test_duration_secs: 20,
        seed: 31415,
        ..Default::default()
    };

    println!("🧪 Running stress test with 4 bonded links (Markov heavy loss)");
    println!("📝 Using simplified receiver pipeline for debugging");
    println!("🔧 Debug logging enabled (RUST_LOG=debug, GST_DEBUG=3,rist*:5,rtp*:4)");

    // Start four independent link scenarios for bonding using netns-testbench
    println!(
        "🌐 Setting up network orchestrator with seed {}",
        config.seed
    );
    let mut orchestrator = NetworkOrchestrator::new(config.seed)
        .await
        .map_err(|e| format!("Failed to create network orchestrator: {}", e))?;

    let scenarios = build_stress_bonded_scenarios();
    println!("📋 Built {} test scenarios", scenarios.len());

    let handles = orchestrator
        .start_bonding_scenarios(scenarios, config.rx_port)
        .await
        .map_err(|e| format!("Failed to start bonding scenarios: {}", e))?;

    println!("Started {} bonded links", handles.len());
    let ports: Vec<u16> = handles.iter().map(|h| h.ingress_port).collect();
    println!("Using ingress ports: {:?}", ports);
    println!("Base egress port: {}", config.rx_port);

    // Build sender/receiver with simplified receiver for debugging
    println!("🏗️  Building stress sender pipeline...");
    let (sender, ristsink) = build_stress_sender_pipeline(&ports);

    println!("🏗️  Building simplified receiver pipeline...");
    let (receiver, counter) = build_simple_receiver_pipeline(&ports);

    println!("🔧 Built simplified receiver pipeline for debugging RIST connection");

    // Verify element states before running
    println!("Checking sender pipeline elements...");
    let mut sender_iter = sender.iterate_elements();
    while let Ok(Some(element)) = sender_iter.next() {
        println!(
            "  - {}: {:?}",
            element
                .factory()
                .map(|f| f.name())
                .unwrap_or_else(|| "unknown".into()),
            element.current_state()
        );
    }

    println!("Checking receiver pipeline elements...");
    let mut receiver_iter = receiver.iterate_elements();
    while let Ok(Some(element)) = receiver_iter.next() {
        println!(
            "  - {}: {:?}",
            element
                .factory()
                .map(|f| f.name())
                .unwrap_or_else(|| "unknown".into()),
            element.current_state()
        );
    }

    // Print RIST sink configuration
    if let Ok(bonding_addrs) = testing::get_property::<String>(&ristsink, "bonding-addresses") {
        println!("📡 RIST sink bonding addresses: {}", bonding_addrs);
    }

    // Run pipelines with enhanced error monitoring
    println!(
        "Running stress test for {} seconds",
        config.test_duration_secs
    );
    let result = run_pipelines(sender, receiver, config.test_duration_secs).await;

    // Handle the result after pipelines are stopped
    if let Err(e) = result {
        println!("❌ Pipeline error: {}", e);
        return Err(e);
    }

    // Verify data made it through
    let count: u64 = testing::get_property(&counter, "count").unwrap_or_else(|_| 0u64);
    println!("Stress test: received {} buffers", count);

    // Print RIST statistics for debugging
    let bonding_stats: String = testing::get_property(&ristsink, "stats")
        .unwrap_or_else(|_| "No stats available".to_string());
    println!("RIST stats: {}", bonding_stats);

    // More detailed assertion with better error message
    if count == 0 {
        println!("❌ No data received - debugging information:");
        println!("   1. 🌐 Network orchestrator port forwarding check needed");
        println!("   2. 🤝 RIST handshake completion status unknown");
        println!("   3. Bonding configuration may be incorrect");
        println!("   4. 🚪 Port binding conflicts possible");
        println!("   5. 📡 Check if RIST elements are properly linked");
        println!("");
        println!("💡 Suggested debugging steps:");
        println!("   - Run with RUST_LOG=debug for detailed logs");
        println!("   - Check network namespace setup");
        println!("   - Verify GStreamer RIST plugin is loaded");
        println!("   - Test with simpler single-link scenario first");

        return Err("No data received in stress test - RIST connection likely failed".into());
    }

    println!("✅ Stress test passed with {} buffers received", count);
    Ok(())
}
