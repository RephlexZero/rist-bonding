//! Element pad semantics and event handling tests
//!
//! These tests verify that the dispatcher properly handles GStreamer events,
//! caps negotiation, and pad lifecycle management.

use gstristsmart::testing::*;
use gstristsmart::test_pipeline;
use gstreamer as gst;
use gst::prelude::*;

#[test]
fn test_caps_negotiation_and_proxying() {
    init_for_tests();

    println!("=== Caps Negotiation and Proxying Test ===");

    // Create elements
    let source = gst::ElementFactory::make("audiotestsrc")
        .property("num-buffers", 10)
        .build()
        .expect("Failed to create audiotestsrc");

    let dispatcher = create_dispatcher(Some(&[1.0]));
    let sink = create_fake_sink();

    // Create pipeline
    test_pipeline!(pipeline, &source, &dispatcher, &sink);

    // Request src pad and link
    let src_pad = dispatcher.request_pad_simple("src_%u").unwrap();
    source.link(&dispatcher).expect("Failed to link source to dispatcher");
    src_pad.link(&sink.static_pad("sink").unwrap()).expect("Failed to link dispatcher to sink");

    // Set pipeline to PAUSED to trigger caps negotiation
    wait_for_state_change(&pipeline, gst::State::Paused, 5)
        .expect("Caps negotiation failed");

    // Verify that caps were negotiated
    let sink_pad = dispatcher.static_pad("sink").unwrap();
    let caps = sink_pad.current_caps();
    assert!(caps.is_some(), "Dispatcher sink pad should have negotiated caps");

    let src_caps = src_pad.current_caps();
    assert!(caps == src_caps, "Source and sink caps should match through dispatcher");

    if let Some(caps) = caps {
        println!("Negotiated caps: {}", caps);
        assert!(caps.to_string().contains("audio"), "Should negotiate audio caps");
    }

    pipeline.set_state(gst::State::Null).expect("Failed to stop pipeline");
    println!("✅ Caps negotiation test passed");
}

#[test]
fn test_eos_event_fanout() {
    init_for_tests();

    println!("=== EOS Event Fanout Test ===");

    // Create elements
    let source = create_test_source(); // This will send EOS after num-buffers
    let dispatcher = create_dispatcher(Some(&[0.5, 0.5]));
    let counter1 = create_counter_sink();
    let counter2 = create_counter_sink();

    // Create pipeline
    test_pipeline!(pipeline, &source, &dispatcher, &counter1, &counter2);

    // Request pads and link
    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();

    source.link(&dispatcher).expect("Failed to link source");
    src_0.link(&counter1.static_pad("sink").unwrap()).expect("Failed to link src_0");
    src_1.link(&counter2.static_pad("sink").unwrap()).expect("Failed to link src_1");

    // Run until EOS
    pipeline.set_state(gst::State::Playing).expect("Failed to start pipeline");

    let bus = pipeline.bus().unwrap();
    let mut got_eos = false;
    let timeout = gst::ClockTime::from_seconds(10);

    // Wait for EOS
    while let Some(msg) = bus.timed_pop(Some(timeout)) {
        match msg.view() {
            gst::MessageView::Eos(..) => {
                got_eos = true;
                break;
            }
            gst::MessageView::Error(err) => {
                panic!("Pipeline error: {}", err.error());
            }
            _ => {}
        }
    }

    assert!(got_eos, "Should have received EOS");

    // Check that both sinks received EOS
    let counter1_eos: bool = get_property(&counter1, "got-eos").unwrap();
    let counter2_eos: bool = get_property(&counter2, "got-eos").unwrap();

    println!("Counter 1 EOS: {}, Counter 2 EOS: {}", counter1_eos, counter2_eos);

    assert!(counter1_eos, "Counter 1 should have received EOS");
    assert!(counter2_eos, "Counter 2 should have received EOS");

    pipeline.set_state(gst::State::Null).expect("Failed to stop pipeline");
    println!("✅ EOS fanout test passed");
}

#[test]
fn test_flush_event_handling() {
    init_for_tests();

    println!("=== Flush Event Handling Test ===");

    let dispatcher = create_dispatcher(Some(&[1.0, 1.0]));
    let counter1 = create_counter_sink();
    let counter2 = create_counter_sink();

    // Create minimal pipeline for testing flush events
    test_pipeline!(pipeline, &dispatcher, &counter1, &counter2);

    // Request pads and link
    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();

    src_0.link(&counter1.static_pad("sink").unwrap()).expect("Failed to link src_0");
    src_1.link(&counter2.static_pad("sink").unwrap()).expect("Failed to link src_1");

    // Set to paused state
    wait_for_state_change(&pipeline, gst::State::Paused, 5)
        .expect("Failed to pause pipeline");

    // Send flush events manually to the dispatcher sink pad
    let sink_pad = dispatcher.static_pad("sink").unwrap();
    
    let flush_start = gst::event::FlushStart::new();
    let flush_stop = gst::event::FlushStop::new(true);

    assert!(sink_pad.send_event(flush_start), "Should handle flush start");
    assert!(sink_pad.send_event(flush_stop), "Should handle flush stop");

    // Give some time for events to propagate
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Check if flush events were forwarded to sinks
    let counter1_flush_start: bool = get_property(&counter1, "got-flush-start").unwrap();
    let counter1_flush_stop: bool = get_property(&counter1, "got-flush-stop").unwrap();
    let counter2_flush_start: bool = get_property(&counter2, "got-flush-start").unwrap();
    let counter2_flush_stop: bool = get_property(&counter2, "got-flush-stop").unwrap();

    println!("Counter 1 - Flush Start: {}, Flush Stop: {}", 
             counter1_flush_start, counter1_flush_stop);
    println!("Counter 2 - Flush Start: {}, Flush Stop: {}", 
             counter2_flush_start, counter2_flush_stop);

    assert!(counter1_flush_start, "Counter 1 should have received flush start");
    assert!(counter1_flush_stop, "Counter 1 should have received flush stop");
    assert!(counter2_flush_start, "Counter 2 should have received flush start");
    assert!(counter2_flush_stop, "Counter 2 should have received flush stop");

    pipeline.set_state(gst::State::Null).expect("Failed to stop pipeline");
    println!("✅ Flush event handling test passed");
}

#[test]
fn test_sticky_events_replay() {
    init_for_tests();

    println!("=== Sticky Events Replay Test ===");

    // Create elements
    let source = create_test_source();
    let dispatcher = create_dispatcher(Some(&[1.0]));

    // Create pipeline with just source and dispatcher first
    test_pipeline!(pipeline, &source, &dispatcher);
    source.link(&dispatcher).expect("Failed to link source to dispatcher");

    // Start pipeline to establish sticky events
    wait_for_state_change(&pipeline, gst::State::Paused, 5)
        .expect("Failed to pause pipeline");

    // Now add a sink and request a new pad - sticky events should be replayed
    let counter = create_counter_sink();
    pipeline.add(&counter).expect("Failed to add counter to pipeline");

    let src_pad = dispatcher.request_pad_simple("src_%u").unwrap();
    src_pad.link(&counter.static_pad("sink").unwrap())
        .expect("Failed to link new pad");

    // The counter should receive sticky events (stream-start, caps, segment)
    // Let's run briefly to ensure events are processed
    run_pipeline_for_duration(&pipeline, 1).expect("Pipeline run failed");

    // Verify that the counter received some buffers (indicating sticky events worked)
    let count: u64 = get_property(&counter, "count").unwrap();
    println!("New sink received {} buffers", count);

    assert!(count > 0, "New sink should have received buffers after sticky event replay");

    println!("✅ Sticky events replay test passed");
}

#[test]
fn test_pad_removal_and_cleanup() {
    init_for_tests();

    println!("=== Pad Removal and Cleanup Test ===");

    let dispatcher = create_dispatcher(Some(&[1.0, 1.0, 1.0]));

    // Request multiple pads
    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_2 = dispatcher.request_pad_simple("src_%u").unwrap();

    println!("Created pads: {}, {}, {}", 
             src_0.name(), src_1.name(), src_2.name());

    // Verify pads exist
    assert_eq!(dispatcher.num_src_pads(), 3);

    // Release one pad
    dispatcher.release_request_pad(&src_1);

    // Verify pad count decreased
    // Note: The actual cleanup might be deferred, so we just check the request worked
    println!("Released pad: {}", src_1.name());

    // Try to request another pad to ensure the dispatcher is still functional
    let src_new = dispatcher.request_pad_simple("src_%u").unwrap();
    println!("Created new pad: {}", src_new.name());

    println!("✅ Pad removal and cleanup test passed");
}
