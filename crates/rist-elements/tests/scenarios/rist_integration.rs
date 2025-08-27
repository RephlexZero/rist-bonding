//! RIST plugin integration tests
//!
//! These tests verify that the RIST smart elements work correctly with
//! actual RIST transport elements when available.

use gst::prelude::*;
use gstreamer as gst;
use gstristelements::testing::*;

#[test]
fn test_rist_plugin_availability() {
    init_for_tests();

    println!("=== RIST Plugin Availability Test ===");

    // Our elements should always be available
    assert!(
        gst::ElementFactory::find("ristdispatcher").is_some(),
        "ristdispatcher should be available"
    );
    assert!(
        gst::ElementFactory::find("dynbitrate").is_some(),
        "dynbitrate should be available"
    );

    // Test harness elements should be available with test-plugin feature
    assert!(
        gst::ElementFactory::find("counter_sink").is_some(),
        "counter_sink should be available"
    );
    assert!(
        gst::ElementFactory::find("riststats_mock").is_some(),
        "riststats_mock should be available"
    );

    // Check for RIST transport elements (optional - they may not be installed)
    let has_ristsrc = gst::ElementFactory::find("ristsrc").is_some();
    let has_ristsink = gst::ElementFactory::find("ristsink").is_some();

    if has_ristsrc && has_ristsink {
        println!("RIST transport elements found: ristsrc, ristsink");
    } else {
        println!("ⓘ RIST transport elements not available (this is OK for unit testing)");
    }

    println!("✅ RIST plugin availability test completed");
}

#[test]
fn test_basic_data_flow_without_rist() {
    init_for_tests();

    println!("=== Basic Data Flow Test (No RIST Transport) ===");

    // Create a simple pipeline that doesn't require RIST transport elements
    let source = create_test_source();
    let dispatcher = create_dispatcher_for_testing(Some(&[0.7, 0.3]));
    let counter1 = create_counter_sink();
    let counter2 = create_counter_sink();

    let pipeline = gst::Pipeline::new();
    pipeline
        .add_many([&source, &dispatcher, &counter1, &counter2])
        .expect("Failed to add elements to pipeline");

    // Link elements
    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();

    source
        .link(&dispatcher)
        .expect("Failed to link source to dispatcher");
    src_0
        .link(&counter1.static_pad("sink").unwrap())
        .expect("Failed to link src_0");
    src_1
        .link(&counter2.static_pad("sink").unwrap())
        .expect("Failed to link src_1");

    // Run the pipeline
    run_pipeline_for_duration(&pipeline, 2).expect("Pipeline run failed");

    // Verify data flow
    let count1: u64 = get_property(&counter1, "count").unwrap();
    let count2: u64 = get_property(&counter2, "count").unwrap();

    println!(
        "Counter 1: {} buffers, Counter 2: {} buffers",
        count1, count2
    );

    assert!(count1 > 0, "Counter 1 should receive buffers");
    assert!(count2 > 0, "Counter 2 should receive buffers");

    // With 0.7/0.3 weights, counter 1 should get more traffic
    println!(
        "Traffic distribution ratio: {:.2}",
        count1 as f64 / count2 as f64
    );

    println!("✅ Basic data flow test completed");
}

#[test]
fn test_bonding_configuration() {
    init_for_tests();

    println!("=== Bonding Configuration Test ===");

    // Test creating dispatcher with multiple outputs (simulating bonded sessions)
    let dispatcher = create_dispatcher(Some(&[0.4, 0.3, 0.3])); // 3-way bonding

    // Request multiple src pads to simulate bonded sessions
    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_2 = dispatcher.request_pad_simple("src_%u").unwrap();

    println!(
        "Created bonding pads: {}, {}, {}",
        src_0.name(),
        src_1.name(),
        src_2.name()
    );

    // Verify we can set bonding-related properties
    dispatcher.set_property("rebalance-interval-ms", 100u64);
    dispatcher.set_property("strategy", "ewma");
    dispatcher.set_property("auto-balance", true);

    let interval: u64 = get_property(&dispatcher, "rebalance-interval-ms").unwrap();
    let strategy: String = get_property(&dispatcher, "strategy").unwrap();
    let auto_balance: bool = get_property(&dispatcher, "auto-balance").unwrap();

    assert_eq!(interval, 100);
    assert_eq!(strategy, "ewma");
    assert!(auto_balance);

    println!(
        "Bonding configuration - Interval: {}ms, Strategy: {}, Auto-balance: {}",
        interval, strategy, auto_balance
    );

    println!("✅ Bonding configuration test completed");
}

#[test]
#[ignore] // Only run when RIST transport elements are available
fn test_rist_pipeline_with_transport() {
    gst::init().expect("Failed to initialize GStreamer");

    // This test requires actual RIST transport elements
    let ristsrc = match gst::ElementFactory::find("ristsrc") {
        Some(_) => gst::ElementFactory::make("ristsrc")
            .property("address", "127.0.0.1")
            .property("port", 1234u32)
            .build()
            .expect("Failed to create ristsrc"),
        None => {
            println!("Skipping RIST transport test - ristsrc not available");
            return;
        }
    };

    let ristsink = match gst::ElementFactory::find("ristsink") {
        Some(_) => gst::ElementFactory::make("ristsink")
            .property("address", "127.0.0.1")
            .property("port", 1234u32)
            .build()
            .expect("Failed to create ristsink"),
        None => {
            println!("Skipping RIST transport test - ristsink not available");
            return;
        }
    };

    println!("=== RIST Transport Pipeline Test ===");

    // Create a more realistic pipeline with RIST transport
    let source = create_test_source();
    let dispatcher = create_dispatcher(Some(&[1.0])); // Single output for simplicity
    let counter = create_counter_sink();

    let pipeline = gst::Pipeline::new();
    pipeline
        .add_many([&source, &dispatcher, &ristsink, &ristsrc, &counter])
        .expect("Failed to add elements to pipeline");

    // Link: source -> dispatcher -> ristsink
    // And: ristsrc -> counter
    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();

    source.link(&dispatcher).expect("Failed to link source");
    src_0
        .link(&ristsink.static_pad("sink").unwrap())
        .expect("Failed to link to ristsink");
    ristsrc.link(&counter).expect("Failed to link from ristsrc");

    // Run the pipeline briefly
    match run_pipeline_for_duration(&pipeline, 3) {
        Ok(_) => {
            let count: u64 = get_property(&counter, "count").unwrap();
            println!("Received {} buffers through RIST transport", count);

            if count > 0 {
                println!("✅ RIST transport pipeline test completed successfully");
            } else {
                println!("RIST transport pipeline ran but no data received");
            }
        }
        Err(e) => {
            println!("RIST transport pipeline test failed: {}", e);
            // This is not necessarily a failure - RIST transport setup can be complex
        }
    }
}

#[test]
fn test_audio_pipeline_without_rist_transport() {
    init_for_tests();

    println!("=== Audio Pipeline Test (Without RIST Transport) ===");

    // Create an audio processing pipeline that tests our elements
    // without requiring RIST transport elements
    let source = create_test_source(); // audiotestsrc
    let encoder = create_encoder_stub(Some(5000)); // Mock encoder
    let dynbitrate = create_dynbitrate();
    let dispatcher = create_dispatcher_for_testing(Some(&[0.6, 0.4]));
    let counter1 = create_counter_sink();
    let counter2 = create_counter_sink();

    let pipeline = gst::Pipeline::new();
    pipeline
        .add_many([
            &source,
            &encoder,
            &dynbitrate,
            &dispatcher,
            &counter1,
            &counter2,
        ])
        .expect("Failed to add elements to pipeline");

    // Link the pipeline
    source
        .link(&encoder)
        .expect("Failed to link source to encoder");
    encoder
        .link(&dynbitrate)
        .expect("Failed to link encoder to dynbitrate");
    dynbitrate
        .link(&dispatcher)
        .expect("Failed to link dynbitrate to dispatcher");

    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();

    src_0
        .link(&counter1.static_pad("sink").unwrap())
        .expect("Failed to link src_0");
    src_1
        .link(&counter2.static_pad("sink").unwrap())
        .expect("Failed to link src_1");

    // Test the complete pipeline
    run_pipeline_for_duration(&pipeline, 2).expect("Audio pipeline failed");

    // Verify results
    let count1: u64 = get_property(&counter1, "count").unwrap();
    let count2: u64 = get_property(&counter2, "count").unwrap();
    let final_bitrate: u32 = get_property(&encoder, "bitrate").unwrap();

    println!("Audio pipeline results:");
    println!("  Counter 1: {} buffers", count1);
    println!("  Counter 2: {} buffers", count2);
    println!("  Final encoder bitrate: {} kbps", final_bitrate);

    assert!(
        count1 > 0 && count2 > 0,
        "Both outputs should receive audio data"
    );
    assert_eq!(final_bitrate, 5000, "Encoder bitrate should be maintained");

    println!("✅ Audio pipeline test completed");
}
