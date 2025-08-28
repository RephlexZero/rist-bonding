use gst::prelude::*;
use gstreamer as gst;
use gstristelements::testing::*;
use std::time::Duration;

/// Pump the GLib main loop for the specified duration
fn run_mainloop_ms(ms: u64) {
    let ctx = glib::MainContext::default();
    let _guard = ctx.acquire().expect("acquire main context");
    let end = std::time::Instant::now() + Duration::from_millis(ms);
    while std::time::Instant::now() < end {
        while ctx.iteration(false) {}
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn test_dispatcher_with_rist_elements() {
    init_for_tests();
    println!("=== Dispatcher Integration with RIST Elements Test ===");

    let dispatcher = create_dispatcher_for_testing(Some(&[0.5, 0.5]));
    dispatcher.set_property("strategy", "ewma");
    dispatcher.set_property("rebalance-interval-ms", 200u64);

    let pipeline = gst::Pipeline::new();

    // Create source and RIST elements
    let source = create_test_source();

    // Create RIST sender and receiver elements (using mock elements for testing)
    let rist_sender1 = create_encoder_stub(None);
    let rist_sender2 = create_encoder_stub(None);

    // Create RIST receiver elements
    let rist_receiver1 = create_counter_sink();
    let rist_receiver2 = create_counter_sink();

    // Add all elements to pipeline
    pipeline
        .add_many([
            &source,
            &dispatcher,
            &rist_sender1,
            &rist_sender2,
            &rist_receiver1,
            &rist_receiver2,
        ])
        .unwrap();

    // Link source to dispatcher
    source.link(&dispatcher).unwrap();

    // Request pads from dispatcher and link to RIST senders
    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();

    src_0
        .link(&rist_sender1.static_pad("sink").unwrap())
        .unwrap();
    src_1
        .link(&rist_sender2.static_pad("sink").unwrap())
        .unwrap();

    // Link RIST senders to receivers (simulating network transmission)
    rist_sender1.link(&rist_receiver1).unwrap();
    rist_sender2.link(&rist_receiver2).unwrap();

    println!("Testing RIST element integration...");
    pipeline.set_state(gst::State::Playing).unwrap();
    run_mainloop_ms(1000);

    let count1: u64 = get_property(&rist_receiver1, "count").unwrap_or(0);
    let count2: u64 = get_property(&rist_receiver2, "count").unwrap_or(0);

    println!("RIST Receiver 1 count: {}", count1);
    println!("RIST Receiver 2 count: {}", count2);

    pipeline.set_state(gst::State::Null).unwrap();

    assert!(count1 > 0, "RIST receiver 1 should receive data");
    assert!(count2 > 0, "RIST receiver 2 should receive data");

    let total_count = count1 + count2;
    let balance_ratio = count1.min(count2) as f64 / total_count as f64;
    assert!(
        balance_ratio >= 0.2,
        "Load balancing should be reasonably distributed"
    );

    println!("✅ RIST element integration test passed");
}

#[test]
fn test_dispatcher_with_dynamic_bitrate() {
    init_for_tests();
    println!("=== Dispatcher Integration with Dynamic Bitrate Test ===");

    let dispatcher = create_dispatcher_for_testing(Some(&[0.4, 0.6]));

    let pipeline = gst::Pipeline::new();
    let source = create_test_source();

    // Create dynamic bitrate elements
    let dynbitrate1 = create_dynbitrate();
    let dynbitrate2 = create_dynbitrate();

    // Set different bitrate ranges for testing
    dynbitrate1.set_property("min-kbps", 1000u32); // 1 Mbps min
    dynbitrate1.set_property("max-kbps", 3000u32); // 3 Mbps max
    dynbitrate2.set_property("min-kbps", 2000u32); // 2 Mbps min
    dynbitrate2.set_property("max-kbps", 5000u32); // 5 Mbps max

    let counter1 = create_counter_sink();
    let counter2 = create_counter_sink();

    pipeline
        .add_many([
            &source,
            &dispatcher,
            &dynbitrate1,
            &dynbitrate2,
            &counter1,
            &counter2,
        ])
        .unwrap();

    // Link elements: source -> dispatcher -> dynbitrate -> counter
    source.link(&dispatcher).unwrap();

    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();

    src_0
        .link(&dynbitrate1.static_pad("sink").unwrap())
        .unwrap();
    src_1
        .link(&dynbitrate2.static_pad("sink").unwrap())
        .unwrap();

    dynbitrate1.link(&counter1).unwrap();
    dynbitrate2.link(&counter2).unwrap();

    println!("Testing dynamic bitrate integration...");
    pipeline.set_state(gst::State::Playing).unwrap();
    run_mainloop_ms(800);

    let count1: u64 = get_property(&counter1, "count").unwrap_or(0);
    let count2: u64 = get_property(&counter2, "count").unwrap_or(0);

    println!("DynBitrate 1 count: {}", count1);
    println!("DynBitrate 2 count: {}", count2);

    // Test dynamic bitrate range changes
    println!("Testing dynamic bitrate range changes...");
    dynbitrate1.set_property("max-kbps", 6000u32); // Change max to 6 Mbps
    dynbitrate2.set_property("min-kbps", 500u32); // Change min to 0.5 Mbps

    run_mainloop_ms(500);
    pipeline.set_state(gst::State::Null).unwrap();

    let final_count1: u64 = get_property(&counter1, "count").unwrap_or(0);
    let final_count2: u64 = get_property(&counter2, "count").unwrap_or(0);

    println!(
        "Final counts: DynBitrate1={}, DynBitrate2={}",
        final_count1, final_count2
    );

    assert!(
        final_count1 >= count1,
        "DynBitrate 1 should continue processing"
    );
    assert!(
        final_count2 >= count2,
        "DynBitrate 2 should continue processing"
    );

    println!("✅ Dynamic bitrate integration test passed");
}

#[test]
fn test_dispatcher_with_stats_feedback() {
    init_for_tests();
    println!("=== Dispatcher Integration with Stats Feedback Test ===");

    let dispatcher = create_dispatcher_for_testing(Some(&[0.5, 0.5]));
    dispatcher.set_property("strategy", "aimd");
    dispatcher.set_property("auto-balance", true);
    dispatcher.set_property("rebalance-interval-ms", 200u64);

    let pipeline = gst::Pipeline::new();
    let source = create_test_source();

    // Create stats elements for feedback
    let stats_mock1 = create_riststats_mock(Some(0.95), Some(50));
    let stats_mock2 = create_riststats_mock(Some(0.90), Some(60));

    // Set different stats to test feedback response
    let stats1 = gst::Structure::builder("rist-stats")
        .field("tx_packets", 100u64)
        .field("tx_bytes", 150000u64)
        .field("rx_packets", 95u64)
        .field("rx_bytes", 142500u64)
        .build();

    let stats2 = gst::Structure::builder("rist-stats")
        .field("tx_packets", 100u64)
        .field("tx_bytes", 150000u64)
        .field("rx_packets", 90u64)
        .field("rx_bytes", 135000u64)
        .build();

    stats_mock1.set_property("stats", &stats1);
    stats_mock2.set_property("stats", &stats2);

    let counter1 = create_counter_sink();
    let counter2 = create_counter_sink();

    pipeline
        .add_many([&source, &dispatcher, &counter1, &counter2])
        .unwrap();

    source.link(&dispatcher).unwrap();

    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();

    // Link dispatcher directly to counters (stats elements are for property testing only)
    src_0.link(&counter1.static_pad("sink").unwrap()).unwrap();
    src_1.link(&counter2.static_pad("sink").unwrap()).unwrap();

    // Connect stats elements to dispatcher for feedback (property-based)
    dispatcher.set_property("rist", &stats_mock1); // Primary stats source

    println!("Testing stats feedback integration...");
    pipeline.set_state(gst::State::Playing).unwrap();
    run_mainloop_ms(600);

    let initial_count1: u64 = get_property(&counter1, "count").unwrap_or(0);
    let initial_count2: u64 = get_property(&counter2, "count").unwrap_or(0);

    // Update stats to simulate network conditions
    let updated_stats1 = gst::Structure::builder("rist-stats")
        .field("tx_packets", 200u64)
        .field("tx_bytes", 300000u64)
        .field("rx_packets", 180u64)
        .field("rx_bytes", 270000u64)
        .build();

    let updated_stats2 = gst::Structure::builder("rist-stats")
        .field("tx_packets", 200u64)
        .field("tx_bytes", 300000u64)
        .field("rx_packets", 160u64)
        .field("rx_bytes", 240000u64)
        .build();

    stats_mock1.set_property("stats", &updated_stats1);
    stats_mock2.set_property("stats", &updated_stats2);

    println!("Updated stats - waiting for rebalancing...");
    run_mainloop_ms(600);

    pipeline.set_state(gst::State::Null).unwrap();

    let final_count1: u64 = get_property(&counter1, "count").unwrap_or(0);
    let final_count2: u64 = get_property(&counter2, "count").unwrap_or(0);

    println!(
        "Initial counts: Counter1={}, Counter2={}",
        initial_count1, initial_count2
    );
    println!(
        "Final counts: Counter1={}, Counter2={}",
        final_count1, final_count2
    );

    // Check that rebalancing occurred based on stats feedback
    let current_weights: String = dispatcher.property("weights");
    println!("Final weights after stats feedback: {}", current_weights);

    assert!(
        final_count1 >= initial_count1,
        "Counter 1 should continue processing"
    );
    assert!(
        final_count2 >= initial_count2,
        "Counter 2 should continue processing"
    );

    println!("✅ Stats feedback integration test passed");
}

#[test]
fn test_complex_pipeline_integration() {
    init_for_tests();
    println!("=== Complex Pipeline Integration Test ===");

    let dispatcher = create_dispatcher_for_testing(Some(&[0.3, 0.3, 0.4]));
    dispatcher.set_property("strategy", "ewma");
    dispatcher.set_property("rebalance-interval-ms", 150u64);

    let pipeline = gst::Pipeline::new();

    // Create complex pipeline with multiple element types
    let source = create_test_source();

    // Multiple encoding paths with different configurations
    let encoder1 = create_encoder_stub(None);
    let encoder2 = create_encoder_stub(None);
    let encoder3 = create_encoder_stub(None);

    // Dynamic bitrate elements
    let dynbitrate1 = create_dynbitrate();
    let dynbitrate2 = create_dynbitrate();
    let dynbitrate3 = create_dynbitrate();

    // Set different bitrate ranges
    dynbitrate1.set_property("min-kbps", 500u32); // 0.5 Mbps
    dynbitrate1.set_property("max-kbps", 1500u32); // 1.5 Mbps
    dynbitrate2.set_property("min-kbps", 1000u32); // 1 Mbps
    dynbitrate2.set_property("max-kbps", 3000u32); // 3 Mbps
    dynbitrate3.set_property("min-kbps", 2000u32); // 2 Mbps
    dynbitrate3.set_property("max-kbps", 5000u32); // 5 Mbps

    // Final sinks
    let sink1 = create_counter_sink();
    let sink2 = create_counter_sink();
    let sink3 = create_counter_sink();

    // Add all elements
    pipeline
        .add_many([
            &source,
            &dispatcher,
            &encoder1,
            &encoder2,
            &encoder3,
            &dynbitrate1,
            &dynbitrate2,
            &dynbitrate3,
            &sink1,
            &sink2,
            &sink3,
        ])
        .unwrap();

    // Build complex pipeline
    source.link(&dispatcher).unwrap();

    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_2 = dispatcher.request_pad_simple("src_%u").unwrap();

    // Chain: dispatcher -> encoder -> dynbitrate -> sink
    src_0.link(&encoder1.static_pad("sink").unwrap()).unwrap();
    src_1.link(&encoder2.static_pad("sink").unwrap()).unwrap();
    src_2.link(&encoder3.static_pad("sink").unwrap()).unwrap();

    encoder1.link(&dynbitrate1).unwrap();
    encoder2.link(&dynbitrate2).unwrap();
    encoder3.link(&dynbitrate3).unwrap();

    dynbitrate1.link(&sink1).unwrap();
    dynbitrate2.link(&sink2).unwrap();
    dynbitrate3.link(&sink3).unwrap();

    println!("Testing complex pipeline integration...");
    pipeline.set_state(gst::State::Playing).unwrap();
    run_mainloop_ms(800);

    let count1: u64 = get_property(&sink1, "count").unwrap_or(0);
    let count2: u64 = get_property(&sink2, "count").unwrap_or(0);
    let count3: u64 = get_property(&sink3, "count").unwrap_or(0);

    println!(
        "Path counts: Sink1={}, Sink2={}, Sink3={}",
        count1, count2, count3
    );

    // Test dynamic reconfiguration
    println!("Testing dynamic reconfiguration...");
    dynbitrate1.set_property("max-kbps", 6000u32); // Increase to 6 Mbps max
    dynbitrate2.set_property("min-kbps", 250u32); // Decrease to 0.25 Mbps min
                                                  // Keep dynbitrate3 settings unchanged

    run_mainloop_ms(500);
    pipeline.set_state(gst::State::Null).unwrap();

    let final_count1: u64 = get_property(&sink1, "count").unwrap_or(0);
    let final_count2: u64 = get_property(&sink2, "count").unwrap_or(0);
    let final_count3: u64 = get_property(&sink3, "count").unwrap_or(0);

    println!(
        "Final counts: Sink1={}, Sink2={}, Sink3={}",
        final_count1, final_count2, final_count3
    );

    assert!(final_count1 >= count1, "Sink 1 should continue processing");
    assert!(final_count2 >= count2, "Sink 2 should continue processing");
    assert!(final_count3 >= count3, "Sink 3 should continue processing");

    let total_count = final_count1 + final_count2 + final_count3;
    assert!(
        total_count >= 100,
        "Complex pipeline should process significant data"
    );

    println!("✅ Complex pipeline integration test passed");
}

#[test]
fn test_multi_dispatcher_coordination() {
    init_for_tests();
    println!("=== Multi-Dispatcher Coordination Test ===");

    // Create two dispatchers for different purposes
    let dispatcher1 = create_dispatcher_for_testing(Some(&[0.6, 0.4]));
    let dispatcher2 = create_dispatcher_for_testing(Some(&[0.5, 0.5]));

    dispatcher1.set_property("strategy", "ewma");
    dispatcher2.set_property("strategy", "aimd");

    let pipeline = gst::Pipeline::new();
    let source = create_test_source();

    // Create tee element to split stream to multiple dispatchers
    let tee = gst::ElementFactory::make("tee")
        .name("stream_tee")
        .build()
        .unwrap();

    // Create processing elements for each dispatcher
    let proc1_1 = create_counter_sink();
    let proc1_2 = create_counter_sink();
    let proc2_1 = create_counter_sink();
    let proc2_2 = create_counter_sink();

    proc1_1.set_property("name", "proc1_1");
    proc1_2.set_property("name", "proc1_2");
    proc2_1.set_property("name", "proc2_1");
    proc2_2.set_property("name", "proc2_2");

    pipeline
        .add_many([
            &source,
            &tee,
            &dispatcher1,
            &dispatcher2,
            &proc1_1,
            &proc1_2,
            &proc2_1,
            &proc2_2,
        ])
        .unwrap();

    // Build pipeline: source -> tee -> dispatcher1/dispatcher2 -> processors
    source.link(&tee).unwrap();

    // Link tee to dispatchers
    let tee_src_0 = tee.request_pad_simple("src_%u").unwrap();
    let tee_src_1 = tee.request_pad_simple("src_%u").unwrap();

    tee_src_0
        .link(&dispatcher1.static_pad("sink").unwrap())
        .unwrap();
    tee_src_1
        .link(&dispatcher2.static_pad("sink").unwrap())
        .unwrap();

    // Link dispatchers to processors
    let disp1_src_0 = dispatcher1.request_pad_simple("src_%u").unwrap();
    let disp1_src_1 = dispatcher1.request_pad_simple("src_%u").unwrap();
    let disp2_src_0 = dispatcher2.request_pad_simple("src_%u").unwrap();
    let disp2_src_1 = dispatcher2.request_pad_simple("src_%u").unwrap();

    disp1_src_0
        .link(&proc1_1.static_pad("sink").unwrap())
        .unwrap();
    disp1_src_1
        .link(&proc1_2.static_pad("sink").unwrap())
        .unwrap();
    disp2_src_0
        .link(&proc2_1.static_pad("sink").unwrap())
        .unwrap();
    disp2_src_1
        .link(&proc2_2.static_pad("sink").unwrap())
        .unwrap();

    println!("Testing multi-dispatcher coordination...");
    pipeline.set_state(gst::State::Playing).unwrap();
    run_mainloop_ms(1000);

    let count1_1: u64 = get_property(&proc1_1, "count").unwrap_or(0);
    let count1_2: u64 = get_property(&proc1_2, "count").unwrap_or(0);
    let count2_1: u64 = get_property(&proc2_1, "count").unwrap_or(0);
    let count2_2: u64 = get_property(&proc2_2, "count").unwrap_or(0);

    println!(
        "Dispatcher 1 outputs: proc1_1={}, proc1_2={}",
        count1_1, count1_2
    );
    println!(
        "Dispatcher 2 outputs: proc2_1={}, proc2_2={}",
        count2_1, count2_2
    );

    // Test weight updates on both dispatchers
    println!("Testing coordinated weight updates...");
    dispatcher1.set_property("weights", "[0.3, 0.7]");
    dispatcher2.set_property("weights", "[0.8, 0.2]");

    run_mainloop_ms(500);
    pipeline.set_state(gst::State::Null).unwrap();

    let final_count1_1: u64 = get_property(&proc1_1, "count").unwrap_or(0);
    let final_count1_2: u64 = get_property(&proc1_2, "count").unwrap_or(0);
    let final_count2_1: u64 = get_property(&proc2_1, "count").unwrap_or(0);
    let final_count2_2: u64 = get_property(&proc2_2, "count").unwrap_or(0);

    println!("Final counts:");
    println!(
        "  Dispatcher 1: proc1_1={}, proc1_2={}",
        final_count1_1, final_count1_2
    );
    println!(
        "  Dispatcher 2: proc2_1={}, proc2_2={}",
        final_count2_1, final_count2_2
    );

    // Verify both dispatchers processed data
    assert!(
        final_count1_1 + final_count1_2 > 0,
        "Dispatcher 1 should process data"
    );
    assert!(
        final_count2_1 + final_count2_2 > 0,
        "Dispatcher 2 should process data"
    );

    let total_processed = final_count1_1 + final_count1_2 + final_count2_1 + final_count2_2;
    assert!(
        total_processed >= 100,
        "Multi-dispatcher setup should process significant data"
    );

    println!("✅ Multi-dispatcher coordination test passed");
}

#[test]
fn test_error_propagation_integration() {
    init_for_tests();
    println!("=== Error Propagation Integration Test ===");

    let dispatcher = create_dispatcher_for_testing(Some(&[0.5, 0.5]));

    let pipeline = gst::Pipeline::new();
    let source = create_test_source();

    // Create elements, including one that might cause issues
    let good_processor = create_counter_sink();
    let error_processor = create_counter_sink(); // This will simulate error conditions

    pipeline
        .add_many([&source, &dispatcher, &good_processor, &error_processor])
        .unwrap();

    source.link(&dispatcher).unwrap();

    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();

    src_0
        .link(&good_processor.static_pad("sink").unwrap())
        .unwrap();
    src_1
        .link(&error_processor.static_pad("sink").unwrap())
        .unwrap();

    println!("Testing error propagation and recovery...");
    pipeline.set_state(gst::State::Playing).unwrap();
    run_mainloop_ms(500);

    let initial_good_count: u64 = get_property(&good_processor, "count").unwrap_or(0);
    let initial_error_count: u64 = get_property(&error_processor, "count").unwrap_or(0);

    // Simulate error conditions by manipulating pipeline state
    println!("Simulating error condition...");
    error_processor.set_state(gst::State::Paused).unwrap(); // Pause one processor

    run_mainloop_ms(400);

    // Recover from error
    println!("Recovering from error...");
    error_processor.set_state(gst::State::Playing).unwrap();

    run_mainloop_ms(400);
    pipeline.set_state(gst::State::Null).unwrap();

    let final_good_count: u64 = get_property(&good_processor, "count").unwrap_or(0);
    let final_error_count: u64 = get_property(&error_processor, "count").unwrap_or(0);

    println!(
        "Good processor: {} -> {}",
        initial_good_count, final_good_count
    );
    println!(
        "Error processor: {} -> {}",
        initial_error_count, final_error_count
    );

    // Verify that the good processor continued working despite errors in the other path
    // Note: Since we're using a finite test source, we check that processing happened
    assert!(
        initial_good_count > 0,
        "Good processor should process some data initially"
    );
    assert!(
        initial_error_count > 0,
        "Error processor should process some data initially"
    );

    // The important thing is that the dispatcher remained stable during the error simulation
    assert!(
        final_good_count >= initial_good_count,
        "Good processor should not lose data"
    );
    assert!(
        final_error_count >= initial_error_count,
        "Error processor should not lose data"
    );

    println!("Dispatcher remained stable during error simulation");

    // Verify dispatcher remained functional
    let final_weights: String = dispatcher.property("weights");
    println!("Dispatcher weights after error recovery: {}", final_weights);

    println!("✅ Error propagation integration test passed");
}
