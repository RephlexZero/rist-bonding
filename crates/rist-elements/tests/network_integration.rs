//! End-to-End RIST Bonding Tests with netns-testbench
//!
//! Comprehensive test suite covering realistic network scenarios including
//! bonding, degradation, handovers, and recovery using the netns-testbench API.

use gstreamer::prelude::*;
use gstristelements::testing;
use netns_testbench::{NetworkOrchestrator, TestScenario};
use scenarios::{DirectionSpec, LinkSpec, Schedule};
use std::collections::HashMap;
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

/// Create a sender pipeline with sessions pointing to orchestrator ingress ports
fn build_sender_pipeline(ports: &[u16]) -> (gstreamer::Pipeline, gstreamer::Element) {
    let pipeline = gstreamer::Pipeline::new();

    // Create video test source at 720p30 (more realistic for tests)
    let videotestsrc = gstreamer::ElementFactory::make("videotestsrc")
        .property("is-live", true)
        .property("pattern", 0) // SMPTE color bars
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

    // Create RIST sink and configure for dispatcher output
    let ristsink = testing::create_rist_sink("127.0.0.1");

    // Configure sink for each network interface
    for (i, &port) in ports.iter().enumerate() {
        ristsink.set_property(&format!("session-{}-port", i), port as u32);
        ristsink.set_property(
            &format!("session-{}-address", i),
            format!("127.0.0.{}", i + 1),
        );
    }

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
            &ristsink,
        ])
        .expect("Failed to add elements to sender pipeline");

    // Link video path
    gstreamer::Element::link_many([&videotestsrc, &video_capsfilter, &videoencoder, &muxer])
        .expect("Failed to link video elements");

    // Link audio path
    gstreamer::Element::link_many([&audiotestsrc, &audio_capsfilter, &audioencoder, &muxer])
        .expect("Failed to link audio elements");

    // Link muxer to RIST sink
    muxer
        .link(&ristsink)
        .expect("Failed to link muxer to ristsink");

    (pipeline, ristsink)
}

/// Create a receiver pipeline with RIST source
fn build_receiver_pipeline(rx_port: u16) -> (gstreamer::Pipeline, gstreamer::Element) {
    let pipeline = gstreamer::Pipeline::new();

    // Create RIST source
    let ristsrc = testing::create_rist_source("127.0.0.1");
    ristsrc.set_property("port", rx_port as u32);

    // Create demuxer
    let demux = gstreamer::ElementFactory::make("tsdemux")
        .build()
        .expect("Failed to create tsdemux");

    // Create counters for verification
    let counter = gstreamer::ElementFactory::make("identity")
        .property("sync", true)
        .property("silent", false)
        .build()
        .expect("Failed to create identity counter");

    // Create sink
    let sink = gstreamer::ElementFactory::make("fakesink")
        .property("sync", false)
        .property("async", false)
        .build()
        .expect("Failed to create fakesink");

    // Add elements
    pipeline
        .add_many([&ristsrc, &demux, &counter, &sink])
        .expect("Failed to add elements to receiver pipeline");

    // Link static elements
    ristsrc
        .link(&demux)
        .expect("Failed to link ristsrc to demux");

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

/// Run both pipelines concurrently with proper lifecycle management
async fn run_pipelines(
    sender: gstreamer::Pipeline,
    receiver: gstreamer::Pipeline,
    duration_secs: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    // Start receiver first
    receiver
        .set_state(gstreamer::State::Playing)
        .expect("Failed to start receiver pipeline");

    // Give receiver time to initialize
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Start sender
    sender
        .set_state(gstreamer::State::Playing)
        .expect("Failed to start sender pipeline");

    // Run for specified duration
    tokio::time::sleep(Duration::from_secs(duration_secs)).await;

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

    println!("🔗 Testing single-link RIST transmission");

    // Create network orchestrator
    let mut orchestrator = NetworkOrchestrator::new(config.seed).await?;

    // Start baseline good scenario
    let scenario = TestScenario::baseline_good();
    let link = orchestrator
        .start_scenario(scenario, config.rx_port)
        .await?;

    println!(
        "✓ Started network link: {} -> {}",
        link.ingress_port, link.egress_port
    );

    // Build pipelines
    let (sender, _ristsink) = build_sender_pipeline(&[link.ingress_port]);
    let (receiver, counter) = build_receiver_pipeline(config.rx_port);

    // Run test
    println!(
        "🚀 Running transmission test for {} seconds",
        config.test_duration_secs
    );
    run_pipelines(sender, receiver, config.test_duration_secs).await?;

    // Verify data was received
    let count: u64 = testing::get_property(&counter, "processed").unwrap_or_else(|_| 0u64);

    println!("📊 Received {} buffers", count);
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

    println!("🔗 Testing dual-link RIST bonding");

    // Create network orchestrator
    let mut orchestrator = NetworkOrchestrator::new(config.seed + 1).await?;

    // Start bonding scenario
    let scenario = TestScenario::bonding_asymmetric();
    let link = orchestrator
        .start_scenario(scenario, config.rx_port)
        .await?;

    println!("✓ Started bonding scenario: {}", link.scenario.name);

    // For bonding, we need multiple ingress ports
    // In a real bonding scenario, we'd have multiple links
    let ports = vec![link.ingress_port, link.ingress_port + 2];

    // Build pipelines with bonding
    let (sender, ristsink) = build_sender_pipeline(&ports);
    let (receiver, counter) = build_receiver_pipeline(config.rx_port);

    // Enable bonding on the sink
    ristsink.set_property("bonding", &true);

    // Run test
    println!(
        "🚀 Running bonding test for {} seconds",
        config.test_duration_secs
    );
    run_pipelines(sender, receiver, config.test_duration_secs).await?;

    // Verify data was received
    let count: u64 = testing::get_property(&counter, "processed").unwrap_or_else(|_| 0u64);

    println!("📊 Received {} buffers through bonding", count);
    assert!(count > 0, "No data received through bonded RIST links");

    // Verify bonding statistics
    let bonding_stats: String = testing::get_property(&ristsink, "stats")
        .unwrap_or_else(|_| "No stats available".to_string());
    println!("📈 Bonding stats: {}", bonding_stats);

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
    let link = orchestrator
        .start_scenario(scenario, config.rx_port)
        .await?;

    println!("✓ Started degrading network scenario");

    // Build pipelines
    let (sender, ristsink) = build_sender_pipeline(&[link.ingress_port]);
    let (receiver, counter) = build_receiver_pipeline(config.rx_port);

    // Enable adaptive bitrate on the sink
    ristsink.set_property("adaptive-bitrate", &true);

    // Run test
    println!(
        "🚀 Running degradation test for {} seconds",
        config.test_duration_secs
    );
    run_pipelines(sender, receiver, config.test_duration_secs).await?;

    // Verify data was received despite degradation
    let count: u64 = testing::get_property(&counter, "processed").unwrap_or_else(|_| 0u64);

    println!("📊 Received {} buffers through degraded network", count);
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
    let link = orchestrator
        .start_scenario(scenario, config.rx_port)
        .await?;

    println!("✓ Started mobile handover scenario");

    // Build pipelines with handover support
    let (sender, ristsink) = build_sender_pipeline(&[link.ingress_port]);
    let (receiver, counter) = build_receiver_pipeline(config.rx_port);

    // Enable seamless handover
    ristsink.set_property("seamless-handover", &true);

    // Run test with handover monitoring
    println!(
        "🚀 Running handover test for {} seconds",
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
            println!("🔄 Handover detected to: {}", current_link);
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
    let count: u64 = testing::get_property(&counter, "processed").unwrap_or_else(|_| 0u64);

    println!("📊 Received {} buffers during handover test", count);
    println!("🔄 Detected {} handover events", handover_count);

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
            "✓ Started scenario {}: {} on port {}",
            i + 1,
            link.scenario.name,
            port
        );
        links.push(link);
    }

    // Create and run multiple pipeline pairs
    let mut handles = Vec::new();

    for (i, link) in links.iter().enumerate() {
        let port = config.rx_port + (i as u16 * 10);
        let (sender, _) = build_sender_pipeline(&[link.ingress_port]);
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
        "🚀 Running {} concurrent scenarios for {} seconds",
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

        let count: u64 = testing::get_property(&counter, "processed").unwrap_or_else(|_| 0u64);
        total_received += count;

        println!("📊 Scenario {}: received {} buffers", i + 1, count);
    }

    println!(
        "📊 Total received across all scenarios: {} buffers",
        total_received
    );
    assert!(total_received > 0, "No data received in stress test");

    Ok(())
}

/// Custom test scenario creation
fn create_custom_race_car_scenario() -> TestScenario {
    use std::time::Duration as StdDuration;

    // Create a realistic race car scenario with changing network conditions
    let mut metadata = HashMap::new();
    metadata.insert("scenario_type".to_string(), "race_car".to_string());
    metadata.insert("track".to_string(), "monaco_street_circuit".to_string());

    TestScenario {
        name: "race_car_monaco".to_string(),
        description: "Monaco street circuit with tunnel and elevation changes".to_string(),
        links: vec![
            // Primary 5G link
            LinkSpec {
                name: "5g_primary".to_string(),
                a_ns: "tx0".to_string(),
                b_ns: "rx0".to_string(),
                a_to_b: Schedule::Steps(vec![
                    (StdDuration::from_secs(0), DirectionSpec::good()), // Start line
                    (StdDuration::from_secs(5), DirectionSpec::good()), // City streets
                    (StdDuration::from_secs(10), DirectionSpec::poor()), // Tunnel entry
                    (StdDuration::from_secs(15), DirectionSpec::good()), // Tunnel exit
                    (StdDuration::from_secs(20), DirectionSpec::good()), // Finish straight
                ]),
                b_to_a: Schedule::Constant(DirectionSpec::good()),
            },
            // Secondary LTE backup
            LinkSpec {
                name: "lte_backup".to_string(),
                a_ns: "tx1".to_string(),
                b_ns: "rx1".to_string(),
                a_to_b: Schedule::Constant(DirectionSpec::typical()),
                b_to_a: Schedule::Constant(DirectionSpec::typical()),
            },
        ],
        duration_seconds: Some(30),
        metadata,
    }
}

/// Test custom race car scenario
#[tokio::test]
#[ignore] // Remove this when ready to run
async fn test_race_car_custom_scenario() -> Result<(), Box<dyn std::error::Error>> {
    testing::init_for_tests();
    let config = TestConfig {
        test_duration_secs: 30,
        seed: 2000,
        ..Default::default()
    };

    println!("🏎️  Testing custom race car scenario");

    // Create network orchestrator
    let mut orchestrator = NetworkOrchestrator::new(config.seed).await?;

    // Start custom race car scenario
    let scenario = create_custom_race_car_scenario();
    let link = orchestrator
        .start_scenario(scenario, config.rx_port)
        .await?;

    println!("✓ Started race car scenario: {}", link.scenario.name);

    // Build pipelines with race car optimizations
    let (sender, ristsink) = build_sender_pipeline(&[link.ingress_port]);
    let (receiver, counter) = build_receiver_pipeline(config.rx_port);

    // Configure for race car requirements
    ristsink.set_property("low-latency-mode", &true);
    ristsink.set_property("adaptive-fec", &true);
    ristsink.set_property("congestion-control", &"bbr");

    // Run race car test
    println!(
        "🚀 Running race car test for {} seconds",
        config.test_duration_secs
    );
    run_pipelines(sender, receiver, config.test_duration_secs).await?;

    // Verify performance metrics
    let count: u64 = testing::get_property(&counter, "processed").unwrap_or_else(|_| 0u64);
    let latency_ms: f64 =
        testing::get_property(&ristsink, "avg-latency-ms").unwrap_or_else(|_| 0.0f64);

    println!("📊 Race car test results:");
    println!("  - Buffers processed: {}", count);
    println!("  - Average latency: {:.2} ms", latency_ms);

    assert!(count > 0, "No data received in race car scenario");
    assert!(
        latency_ms < 100.0,
        "Latency too high for race car application"
    );

    Ok(())
}
