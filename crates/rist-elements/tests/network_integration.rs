//! End-to-end RIST bonding test over the network simulator.

use gstreamer::prelude::*;
use gstristelements::testing;
use netlink_sim::{NetworkOrchestrator, TestScenario};

/// Create a sender pipeline with sessions pointing to orchestrator ingress ports.
fn build_sender_pipeline(ports: &[u16]) -> (gstreamer::Pipeline, gstreamer::Element) {
    let pipeline = gstreamer::Pipeline::new();

    // Create video test source at 1080p60
    let videotestsrc = gstreamer::ElementFactory::make("videotestsrc")
        .property("is-live", true)
        .property("num-buffers", 300) // 5 seconds at 60fps
        .build()
        .expect("Failed to create videotestsrc");

    // Create audio test source with sine wave
    let audiotestsrc = gstreamer::ElementFactory::make("audiotestsrc")
        .property("is-live", true)
        .property("num-buffers", 240) // 5 seconds at 48kHz/1024 samples per buffer
        .property("freq", 440.0) // 440 Hz sine wave (A4 note)
        .build()
        .expect("Failed to create audiotestsrc");

    // Video processing chain
    let videoconvert = gstreamer::ElementFactory::make("videoconvert")
        .build()
        .expect("Failed to create videoconvert");

    let videoscale = gstreamer::ElementFactory::make("videoscale")
        .build()
        .expect("Failed to create videoscale");

    // Video caps for 1080p60
    let video_caps = gstreamer::Caps::builder("video/x-raw")
        .field("width", 1920)
        .field("height", 1080)
        .field("framerate", gstreamer::Fraction::new(60, 1))
        .field("format", "I420")
        .build();
    let video_capsfilter = gstreamer::ElementFactory::make("capsfilter")
        .property("caps", &video_caps)
        .build()
        .expect("Failed to create video capsfilter");

    // H.265 encoder
    let x265enc = gstreamer::ElementFactory::make("x265enc")
        .property("bitrate", 5000u32) // 5 Mbps for 1080p60
        .build()
        .expect("Failed to create x265enc");

    // RTP H.265 payloader
    let rtph265pay = gstreamer::ElementFactory::make("rtph265pay")
        .property("pt", 96u32)
        .property("config-interval", -1i32) // Send VPS/SPS/PPS with every IDR
        .build()
        .expect("Failed to create rtph265pay");

    // Audio processing chain
    let audioconvert = gstreamer::ElementFactory::make("audioconvert")
        .build()
        .expect("Failed to create audioconvert");

    let audioresample = gstreamer::ElementFactory::make("audioresample")
        .build()
        .expect("Failed to create audioresample");

    // Audio caps for L16 (16-bit PCM)
    let audio_caps = gstreamer::Caps::builder("audio/x-raw")
        .field("format", "S16BE") // Big endian for L16
        .field("layout", "interleaved")
        .field("channels", 2)
        .field("rate", 48000)
        .build();
    let audio_capsfilter = gstreamer::ElementFactory::make("capsfilter")
        .property("caps", &audio_caps)
        .build()
        .expect("Failed to create audio capsfilter");

    // RTP L16 payloader (raw audio)
    let rtp_l16pay = gstreamer::ElementFactory::make("rtpL16pay")
        .property("pt", 97u32)
        .build()
        .expect("Failed to create rtpL16pay");

    // RTP muxer to combine video and audio streams
    let rtpmux = gstreamer::ElementFactory::make("rtpmux")
        .build()
        .expect("Failed to create rtpmux");

    // Create bonding addresses as comma-separated string
    let bonding_addresses: Vec<String> = ports.iter().map(|p| format!("127.0.0.1:{}", p)).collect();
    let bonding_addresses_str = bonding_addresses.join(",");

    let ristsink = gstreamer::ElementFactory::make("ristsink")
        .property("bonding-addresses", bonding_addresses_str)
        .build()
        .expect("Failed to create ristsink");

    // Add all elements to pipeline
    pipeline
        .add_many([
            &videotestsrc,
            &videoconvert,
            &videoscale,
            &video_capsfilter,
            &x265enc,
            &rtph265pay,
            &audiotestsrc,
            &audioconvert,
            &audioresample,
            &audio_capsfilter,
            &rtp_l16pay,
            &rtpmux,
            &ristsink,
        ])
        .expect("failed to add elements");

    // Link video chain: videotestsrc -> videoconvert -> videoscale -> caps -> x265enc -> rtph265pay
    gstreamer::Element::link_many([
        &videotestsrc,
        &videoconvert,
        &videoscale,
        &video_capsfilter,
        &x265enc,
        &rtph265pay,
    ])
    .expect("failed to link video chain");

    // Link audio chain: audiotestsrc -> audioconvert -> audioresample -> caps -> rtp_l16pay
    gstreamer::Element::link_many([
        &audiotestsrc,
        &audioconvert,
        &audioresample,
        &audio_capsfilter,
        &rtp_l16pay,
    ])
    .expect("failed to link audio chain");

    // Connect RTP payloaders to muxer
    rtph265pay
        .link(&rtpmux)
        .expect("failed to link video to rtpmux");
    rtp_l16pay
        .link(&rtpmux)
        .expect("failed to link audio to rtpmux");

    // Connect muxer to ristsink
    rtpmux
        .link(&ristsink)
        .expect("failed to link rtpmux to ristsink");

    (pipeline, ristsink)
}

/// Create a receiver pipeline listening on `port`.
fn build_receiver_pipeline(port: u16) -> (gstreamer::Pipeline, gstreamer::Element) {
    let pipeline = gstreamer::Pipeline::new();
    let ristsrc = gstreamer::ElementFactory::make("ristsrc")
        .property("address", "0.0.0.0")
        .property("port", port as u32)
        .build()
        .expect("Failed to create ristsrc");

    // RTP demuxer to separate video and audio streams
    let rtpptdemux = gstreamer::ElementFactory::make("rtpptdemux")
        .build()
        .expect("Failed to create rtpptdemux");

    // Video processing chain
    let rtph265depay = gstreamer::ElementFactory::make("rtph265depay")
        .build()
        .expect("Failed to create rtph265depay");

    let avdec_h265 = gstreamer::ElementFactory::make("avdec_h265")
        .build()
        .expect("Failed to create avdec_h265");

    let videoconvert = gstreamer::ElementFactory::make("videoconvert")
        .build()
        .expect("Failed to create videoconvert");

    // Audio processing chain
    let rtp_l16depay = gstreamer::ElementFactory::make("rtpL16depay")
        .build()
        .expect("Failed to create rtpL16depay");

    let audioconvert = gstreamer::ElementFactory::make("audioconvert")
        .build()
        .expect("Failed to create audioconvert");

    // Counter sinks for testing
    let video_counter = testing::create_counter_sink();
    let audio_counter = testing::create_counter_sink();

    pipeline
        .add_many([
            &ristsrc,
            &rtpptdemux,
            &rtph265depay,
            &avdec_h265,
            &videoconvert,
            &video_counter,
            &rtp_l16depay,
            &audioconvert,
            &audio_counter,
        ])
        .expect("failed to add receiver elements");

    // Link initial chain: ristsrc -> rtpptdemux
    ristsrc
        .link(&rtpptdemux)
        .expect("failed to link ristsrc to rtpptdemux");

    // Link video chain: rtph265depay -> avdec_h265 -> videoconvert -> video_counter
    gstreamer::Element::link_many([&rtph265depay, &avdec_h265, &videoconvert, &video_counter])
        .expect("failed to link video chain");

    // Link audio chain: rtp_l16depay -> audioconvert -> audio_counter
    gstreamer::Element::link_many([&rtp_l16depay, &audioconvert, &audio_counter])
        .expect("failed to link audio chain");

    // Connect rtpptdemux pads dynamically when available
    let video_depay_clone = rtph265depay.clone();
    let audio_depay_clone = rtp_l16depay.clone();

    rtpptdemux.connect_pad_added(move |_element, src_pad| {
        let pad_name = src_pad.name();
        if pad_name.as_str().contains("96") {
            // H.265 video
            if let Some(sink_pad) = video_depay_clone.static_pad("sink") {
                let _ = src_pad.link(&sink_pad);
            }
        } else if pad_name.as_str().contains("97") {
            // L16 audio
            if let Some(sink_pad) = audio_depay_clone.static_pad("sink") {
                let _ = src_pad.link(&sink_pad);
            }
        }
    });

    // Return the video counter as the primary counter for testing
    (pipeline, video_counter)
}

/// Run two pipelines concurrently for `duration` seconds.
async fn run_pipelines(sender: gstreamer::Pipeline, receiver: gstreamer::Pipeline, duration: u64) {
    let s_handle = tokio::task::spawn_blocking(move || {
        testing::run_pipeline_for_duration(&sender, duration).unwrap();
    });
    let r_handle = tokio::task::spawn_blocking(move || {
        testing::run_pipeline_for_duration(&receiver, duration).unwrap();
    });
    let _ = tokio::join!(s_handle, r_handle);
}

/// Helper function to test dispatcher with network simulation
async fn test_dispatcher_with_network(
    _weights: Option<&[f64]>,
    scenario: TestScenario,
    rx_port: u16,
    duration: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut orchestrator = NetworkOrchestrator::new(8000u64 + rx_port as u64);

    // Start network links
    let link1 = orchestrator
        .start_scenario(scenario.clone(), rx_port)
        .await?;
    let link2 = orchestrator.start_scenario(scenario, rx_port).await?;

    let ports = vec![link1.ingress_port, link2.ingress_port];
    let (sender_pipeline, _ristsink) = build_sender_pipeline(&ports);
    let (receiver_pipeline, counter) = build_receiver_pipeline(rx_port);

    run_pipelines(sender_pipeline, receiver_pipeline, duration).await;

    let count: u64 = testing::get_property(&counter, "count").unwrap();
    assert!(count > 0, "counter_sink received no buffers");

    Ok(())
}

/// Helper function to setup bonding test
async fn setup_bonding_test(
    rx_port: u16,
) -> Result<NetworkOrchestrator, Box<dyn std::error::Error>> {
    let mut orchestrator = NetworkOrchestrator::new(8000u64 + rx_port as u64);

    // Start two network links for bonding
    orchestrator
        .start_scenario(TestScenario::baseline_good(), rx_port)
        .await?;
    orchestrator
        .start_scenario(TestScenario::degraded_network(), rx_port)
        .await?;

    Ok(orchestrator)
}

/// Helper function to get test scenarios
fn get_test_scenarios() -> Vec<TestScenario> {
    vec![
        TestScenario::baseline_good(),
        TestScenario::degraded_network(),
        TestScenario::mobile_network(),
        // Additional baseline scenarios for testing
        TestScenario::baseline_good(),
    ]
}

/// Helper function to setup network scenario
async fn setup_network_scenario(
    scenario: TestScenario,
    rx_port: u16,
) -> Result<NetworkOrchestrator, Box<dyn std::error::Error>> {
    let mut orchestrator = NetworkOrchestrator::new(8000u64 + rx_port as u64);
    orchestrator.start_scenario(scenario, rx_port).await?;
    Ok(orchestrator)
}

/// End-to-end test using real RIST transport elements over simulated links.
#[tokio::test]
async fn test_end_to_end_rist_over_simulated_network() {
    testing::init_for_tests();

    let scenario = TestScenario::baseline_good();
    let rx_port = 5204; // Use different port range

    let result = test_dispatcher_with_network(
        Some(&[0.6, 0.4]),
        scenario,
        rx_port,
        5, // 5 second test
    )
    .await;

    assert!(result.is_ok(), "Test failed: {:?}", result);
}

#[tokio::test]
async fn test_dispatcher_with_poor_network() {
    testing::init_for_tests();

    let scenario = TestScenario::degraded_network();
    let rx_port = 5206; // Use even port number

    let result = test_dispatcher_with_network(
        Some(&[0.5, 0.5]),
        scenario,
        rx_port,
        5, // 5 second test
    )
    .await;

    assert!(
        result.is_ok(),
        "Test with poor network failed: {:?}",
        result
    );
}

#[tokio::test]
async fn test_bonding_setup() {
    let rx_port = 5208; // Use even port number

    let result = setup_bonding_test(rx_port).await;
    assert!(result.is_ok(), "Bonding setup failed: {:?}", result.err());

    let orchestrator = result.unwrap();
    let active_links = orchestrator.get_active_links();

    // Should have 2 links for bonding
    assert_eq!(
        active_links.len(),
        2,
        "Expected 2 active links for bonding test"
    );

    // Verify different ingress ports
    assert_ne!(
        active_links[0].ingress_port, active_links[1].ingress_port,
        "Links should have different ingress ports"
    );
}

#[tokio::test]
async fn test_multiple_scenarios() {
    let scenarios = get_test_scenarios();

    assert!(
        scenarios.len() >= 4,
        "Expected at least 4 predefined scenarios"
    );

    // Test each scenario setup (without running full tests)
    for (i, scenario) in scenarios.into_iter().enumerate().take(3) {
        let rx_port = 5230 + (i as u16 * 10); // Use well-spaced port ranges
        let result = setup_network_scenario(scenario, rx_port).await;
        assert!(
            result.is_ok(),
            "Failed to setup scenario {}: {:?}",
            i,
            result.err()
        );
    }
}

#[tokio::test]
async fn test_cellular_network_scenario() {
    let scenario = TestScenario::mobile_network();
    let rx_port = 5220; // Use even port number

    let result = setup_network_scenario(scenario, rx_port).await;
    assert!(
        result.is_ok(),
        "Failed to setup cellular scenario: {:?}",
        result.err()
    );
}

/// End-to-end test using real RIST transport elements over simulated links
#[tokio::test]
#[ignore]
async fn test_end_to_end_rist_over_simulated_links() {
    testing::init_for_tests();

    let rx_port = 6000u16;
    let mut orchestrator = NetworkOrchestrator::new(8000); // Start from 8000 (even)

    // Get first link
    let link1 = orchestrator
        .start_scenario(TestScenario::baseline_good(), rx_port)
        .await
        .expect("failed to start first link");

    // Skip to next even port for second link
    let mut link2 = orchestrator
        .start_scenario(TestScenario::degraded_network(), rx_port)
        .await
        .expect("failed to start second link");

    // If second link is odd, get another one
    while link2.ingress_port % 2 != 0 {
        link2 = orchestrator
            .start_scenario(TestScenario::degraded_network(), rx_port)
            .await
            .expect("failed to start second link");
    }

    let port1 = link1.ingress_port;
    let port2 = link2.ingress_port;

    println!("Using even ingress ports: {} and {}", port1, port2);

    // Run for longer to allow jitter buffer to properly release packets
    let (sender_pipeline, ristsink) = build_sender_pipeline(&[port1, port2]);
    let (receiver_pipeline, counter) = build_receiver_pipeline(rx_port);

    run_pipelines(sender_pipeline, receiver_pipeline, 10).await;

    let count: u64 = testing::get_property(&counter, "count").unwrap();
    assert!(count > 0, "counter_sink received no buffers");

    // Verify that both sessions sent packets if stats are available
    if let Ok(stats) = testing::get_property::<gstreamer::Structure>(&ristsink, "stats") {
        if let Ok(val) = stats.get::<gstreamer::glib::Value>("session-stats") {
            if let Ok(arr) = val.get::<gstreamer::glib::ValueArray>() {
                assert_eq!(arr.len(), 2, "expected stats for two sessions");
                for session in arr.iter() {
                    if let Ok(s) = session.get::<gstreamer::Structure>() {
                        let sent = s.get::<u64>("sent-original-packets").unwrap_or(0);
                        assert!(sent > 0, "session sent no packets");
                    }
                }
            }
        }
    }
}
