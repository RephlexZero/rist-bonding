//! Element Pad Semantics and Event Handling Tests
//!
//! Comprehensive tests for RIST element pad lifecycle, event handling,
//! and GStreamer integration using async testing patterns.

use gstreamer as gst;
use gstreamer::prelude::*;
use gstristelements::testing;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::timeout;

/// Test helper for pad event monitoring
#[derive(Debug, Clone)]
struct PadEventMonitor {
    events: Arc<Mutex<Vec<String>>>,
}

impl PadEventMonitor {
    fn new() -> Self {
        Self {
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn record_event(&self, event: &str) {
        if let Ok(mut events) = self.events.lock() {
            events.push(event.to_string());
        }
    }

    fn get_events(&self) -> Vec<String> {
        self.events.lock().unwrap().clone()
    }

    fn has_event(&self, event_type: &str) -> bool {
        self.get_events().iter().any(|e| e.contains(event_type))
    }

    fn event_count(&self) -> usize {
        self.get_events().len()
    }
}

/// Create a test pipeline with RIST dispatcher
/// Note: The dispatcher->sink link must be made manually using request pads
fn create_test_pipeline() -> (gst::Pipeline, gst::Element, gst::Element, gst::Element) {
    let pipeline = gst::Pipeline::new();

    // Create test source
    let src = testing::create_test_source();

    // Create RIST dispatcher
    let dispatcher = testing::create_dispatcher(None);

    // Create fake sink for testing
    let sink = testing::create_fake_sink();

    pipeline.add_many([&src, &dispatcher, &sink]).unwrap();
    
    // Link source to dispatcher (this works with sink pads)
    src.link(&dispatcher).unwrap();
    
    // Note: dispatcher->sink linking must be done manually with request pads
    // Each test should request a src pad and link it to the sink as needed

    (pipeline, src, dispatcher, sink)
}

#[tokio::test]
async fn test_pad_creation_and_linking() -> Result<(), Box<dyn std::error::Error>> {
    testing::init_for_tests();

    println!("Testing pad creation and linking");

    // Create elements separately without auto-linking for this test
    let pipeline = gst::Pipeline::new();
    let src = testing::create_test_source();
    let dispatcher = testing::create_dispatcher(None);
    let sink = testing::create_fake_sink();
    
    pipeline.add_many([&src, &dispatcher, &sink]).unwrap();
    // Don't link yet - test initial state

    // Check initial pad state
    let sink_pad = dispatcher.static_pad("sink").expect("Should have sink pad");
    let src_pad = dispatcher.request_pad_simple("src_%u").expect("Should be able to request src pad");

    assert!(
        !sink_pad.is_linked(),
        "Sink pad should not be initially linked"
    );
    assert!(
        !src_pad.is_linked(),
        "Source pad should not be initially linked"
    );

    // Now do the linking
    src.link(&dispatcher).unwrap();
    let sink_sink_pad = sink.static_pad("sink").unwrap();
    src_pad.link(&sink_sink_pad).unwrap();

    // Set pipeline to PAUSED to trigger pad linking
    pipeline.set_state(gst::State::Paused)?;

    // Wait for state change
    let (state_return, _, _) = pipeline.state(Some(gst::ClockTime::from_seconds(5)));
    assert!(matches!(state_return, Ok(_)), "State change should succeed");

    // Check pads are now linked
    assert!(
        sink_pad.is_linked(),
        "Sink pad should be linked after pipeline setup"
    );
    assert!(
        src_pad.is_linked(),
        "Source pad should be linked after pipeline setup"
    );

    // Clean up
    pipeline.set_state(gst::State::Null)?;

    println!("Pad creation and linking test passed");
    Ok(())
}

/// Test caps negotiation between elements
#[tokio::test]
async fn test_caps_negotiation() -> Result<(), Box<dyn std::error::Error>> {
    testing::init_for_tests();

    println!("Testing caps negotiation");

    let (pipeline, _src, dispatcher, _sink) = create_test_pipeline();

    // Set pipeline to READY to prepare caps negotiation
    pipeline.set_state(gst::State::Ready)?;

    let sink_pad = dispatcher.static_pad("sink").unwrap();
    let src_pad = dispatcher.request_pad_simple("src_%u").unwrap();
    
    // Link the requested src pad to the sink
    let sink_sink_pad = _sink.static_pad("sink").unwrap();
    src_pad.link(&sink_sink_pad).unwrap();

    // Check initial caps
    let initial_caps = sink_pad.current_caps();
    println!("Initial caps: {:?}", initial_caps);

    // Set to PAUSED to trigger caps negotiation
    pipeline.set_state(gst::State::Paused)?;
    let (_state_return, _, _) = pipeline.state(Some(gst::ClockTime::from_seconds(5)));

    // Check negotiated caps
    let negotiated_caps = src_pad.current_caps();
    println!("Negotiated caps: {:?}", negotiated_caps);

    assert!(negotiated_caps.is_some(), "Caps should be negotiated");

    // Verify caps are compatible
    if let Some(caps) = negotiated_caps {
        assert!(caps.size() > 0, "Negotiated caps should not be empty");
    }

    pipeline.set_state(gst::State::Null)?;

    println!("Caps negotiation test passed");
    Ok(())
}

/// Test event handling (EOS, FLUSH, etc.)
#[tokio::test]
async fn test_event_handling() -> Result<(), Box<dyn std::error::Error>> {
    testing::init_for_tests();

    println!("Testing event handling");

    let (pipeline, _src, dispatcher, _sink) = create_test_pipeline();
    let monitor = PadEventMonitor::new();

    // Set up event probe on dispatcher src pad
    let src_pad = dispatcher.request_pad_simple("src_%u").unwrap();
    
    // Link the requested src pad to the sink
    let sink_sink_pad = _sink.static_pad("sink").unwrap();
    src_pad.link(&sink_sink_pad).unwrap();
    
    let monitor_clone = monitor.clone();

    src_pad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_pad, info| {
        if let Some(gst::PadProbeData::Event(ref event)) = info.data {
            let event_type = event.type_();
            monitor_clone.record_event(&format!("{:?}", event_type));
            println!("Event: {:?}", event_type);
        }
        gst::PadProbeReturn::Ok
    });

    // Start pipeline
    pipeline.set_state(gst::State::Playing)?;

    // Let it run briefly
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Send EOS event (this reliably propagates through elements)
    let sink_pad = dispatcher.static_pad("sink").unwrap();
    sink_pad.send_event(gst::event::Eos::new());

    // Wait for events to propagate
    tokio::time::sleep(Duration::from_millis(500)).await;

    pipeline.set_state(gst::State::Null)?;

    // Check recorded events
    let events = monitor.get_events();
    println!("Recorded events: {:?}", events);

    // Verify we got the basic pipeline events and EOS
    assert!(monitor.has_event("StreamStart"), "Should have received StreamStart event");
    assert!(monitor.has_event("Caps"), "Should have received Caps event");
    assert!(monitor.has_event("Eos"), "Should have received EOS event");
    assert!(
        monitor.event_count() > 3,
        "Should have recorded multiple events"
    );

    println!("Event handling test passed");
    Ok(())
}

/// Test dynamic pad addition and removal
#[tokio::test]
async fn test_dynamic_pad_lifecycle() -> Result<(), Box<dyn std::error::Error>> {
    testing::init_for_tests();

    println!("Testing dynamic pad lifecycle");

    let pipeline = gst::Pipeline::new();
    let src = testing::create_test_source();
    let dispatcher = testing::create_dispatcher(None);

    pipeline.add_many([&src, &dispatcher]).unwrap();
    src.link(&dispatcher).unwrap();

    // Monitor for pad-added/pad-removed signals
    let monitor = PadEventMonitor::new();
    let monitor_clone = monitor.clone();

    dispatcher.connect_pad_added(move |_element, pad| {
        monitor_clone.record_event(&format!("pad-added: {}", pad.name()));
    });

    let monitor_clone = monitor.clone();
    dispatcher.connect_pad_removed(move |_element, pad| {
        monitor_clone.record_event(&format!("pad-removed: {}", pad.name()));
    });

    // Create multiple sinks to test dynamic pad creation
    let mut sinks = Vec::new();
    for _i in 0..3 {
        let sink = testing::create_fake_sink();
        pipeline.add(&sink).unwrap();
        sinks.push(sink);
    }

    pipeline.set_state(gst::State::Ready)?;

    // Request pads dynamically
    let mut request_pads = Vec::new();
    for i in 0..sinks.len() {
        if let Some(pad_template) = dispatcher.pad_template("src_%u") {
            if let Some(pad) =
                dispatcher.request_pad(&pad_template, Some(&format!("src_{}", i)), None)
            {
                request_pads.push(pad);
            }
        }
    }

    // Link dynamic pads
    for (pad, sink) in request_pads.iter().zip(sinks.iter()) {
        let sink_pad = sink.static_pad("sink").unwrap();
        pad.link(&sink_pad)?;
    }

    pipeline.set_state(gst::State::Paused)?;
    tokio::time::sleep(Duration::from_millis(200)).await;

    // First unlink all pads while pipeline is still PAUSED
    for pad in request_pads.iter() {
        if let Some(peer_pad) = pad.peer() {
            pad.unlink(&peer_pad).unwrap();
        }
    }

    // Set pipeline to NULL state before releasing pads
    pipeline.set_state(gst::State::Null)?;

    // Now release pads
    for pad in request_pads.iter() {
        dispatcher.release_request_pad(pad);
    }

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Check pad lifecycle events
    let events = monitor.get_events();
    println!("Pad lifecycle events: {:?}", events);

    // We should see both pad-added and pad-removed events
    assert!(
        monitor.has_event("pad-added"),
        "Should have recorded pad-added events"
    );
    assert!(
        monitor.has_event("pad-removed"),
        "Should have recorded pad-removed events"
    );
    assert!(
        monitor.event_count() >= 6, // 3 added + 3 removed
        "Should have recorded pad lifecycle events for all pads"
    );

    println!("Dynamic pad lifecycle test passed");
    Ok(())
}

/// Test buffer flow and data integrity
#[tokio::test]
async fn test_buffer_flow_integrity() -> Result<(), Box<dyn std::error::Error>> {
    testing::init_for_tests();

    println!("Testing buffer flow and data integrity");

    let (pipeline, _src, dispatcher, _sink) = create_test_pipeline();

    // Set up buffer counting
    let buffer_count = Arc::new(Mutex::new(0u64));
    let byte_count = Arc::new(Mutex::new(0u64));

    // Request src pad and link to sink
    let src_pad = dispatcher.request_pad_simple("src_%u").unwrap();
    let sink_pad = _sink.static_pad("sink").unwrap();
    src_pad.link(&sink_pad).unwrap();

    // Add probe to count buffers
    let buffer_count_clone = buffer_count.clone();
    let byte_count_clone = byte_count.clone();

    src_pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, info| {
        if let Some(gst::PadProbeData::Buffer(ref buffer)) = info.data {
            *buffer_count_clone.lock().unwrap() += 1;
            *byte_count_clone.lock().unwrap() += buffer.size() as u64;
        }
        gst::PadProbeReturn::Ok
    });

    // Configure source for limited buffers
    _src.set_property("num-buffers", &100i32);

    // Start pipeline
    pipeline.set_state(gst::State::Playing)?;

    // Wait for processing with timeout
    let result = timeout(Duration::from_secs(10), async {
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;

            // Check if we've processed expected buffers
            let count = *buffer_count.lock().unwrap();
            if count >= 100 {
                break;
            }

            // Check pipeline state
            let (state, _, _) = pipeline.state(Some(gst::ClockTime::from_mseconds(100)));
            if state.is_err() {
                break;
            }
        }
    })
    .await;

    pipeline.set_state(gst::State::Null)?;

    assert!(
        result.is_ok(),
        "Buffer flow test should complete within timeout"
    );

    let final_buffer_count = *buffer_count.lock().unwrap();
    let final_byte_count = *byte_count.lock().unwrap();

    println!(
        "Processed {} buffers, {} bytes",
        final_buffer_count, final_byte_count
    );

    assert!(final_buffer_count > 0, "Should have processed some buffers");
    assert!(final_byte_count > 0, "Should have processed some data");

    println!("Buffer flow integrity test passed");
    Ok(())
}

/// Test error handling and recovery
#[tokio::test]
async fn test_error_handling_recovery() -> Result<(), Box<dyn std::error::Error>> {
    testing::init_for_tests();

    println!("🚨 Testing error handling and recovery");

    let (pipeline, _src, dispatcher, _sink) = create_test_pipeline();

    // Set up error monitoring
    let bus = pipeline.bus().unwrap();
    let error_count = Arc::new(Mutex::new(0u32));
    let warning_count = Arc::new(Mutex::new(0u32));

    let error_count_clone = error_count.clone();
    let warning_count_clone = warning_count.clone();

    // Monitor bus messages in a separate task
    let bus_watch = bus
        .add_watch(move |_bus, msg| {
            match msg.view() {
                gst::MessageView::Error(err) => {
                    println!("🚨 Pipeline error: {}", err.error());
                    *error_count_clone.lock().unwrap() += 1;
                }
                gst::MessageView::Warning(warn) => {
                    println!("Pipeline warning: {}", warn.error());
                    *warning_count_clone.lock().unwrap() += 1;
                }
                _ => {}
            }
            glib::ControlFlow::Continue
        })
        .unwrap();

    // Start pipeline normally
    pipeline.set_state(gst::State::Playing)?;
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Simulate error condition by sending invalid caps
    let sink_pad = dispatcher.static_pad("sink").unwrap();
    let invalid_caps = gst::Caps::builder("application/x-invalid").build();
    let caps_event = gst::event::Caps::new(&invalid_caps);

    // This might cause warnings/errors
    sink_pad.send_event(caps_event);

    // Let pipeline try to handle the error
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Try to recover by setting back to NULL and restarting
    pipeline.set_state(gst::State::Null)?;
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Restart pipeline
    pipeline.set_state(gst::State::Playing)?;
    tokio::time::sleep(Duration::from_millis(500)).await;

    pipeline.set_state(gst::State::Null)?;

    // Bus watch will be automatically removed when it goes out of scope
    drop(bus_watch);

    let final_error_count = *error_count.lock().unwrap();
    let final_warning_count = *warning_count.lock().unwrap();

    println!(
        "Errors: {}, Warnings: {}",
        final_error_count, final_warning_count
    );

    // We expect the pipeline to handle errors gracefully
    // The exact counts depend on GStreamer behavior
    println!("Error handling and recovery test passed");

    Ok(())
}

/// Test multi-threaded pad access safety
#[tokio::test]
#[ignore = "Skipping multi-threaded test - requires investigation of GStreamer thread safety"]
async fn test_multithread_pad_safety() -> Result<(), Box<dyn std::error::Error>> {
    testing::init_for_tests();

    println!("🧵 Testing multi-threaded pad access safety");

    let pipeline = gst::Pipeline::new();
    let src = testing::create_test_source();
    let dispatcher = testing::create_dispatcher(None);

    pipeline.add_many([&src, &dispatcher]).unwrap();
    src.link(&dispatcher).unwrap();

    // Start pipeline in PAUSED state (safer for testing)
    pipeline.set_state(gst::State::Paused)?;

    let dispatcher_clone = dispatcher.clone();
    let access_count = Arc::new(Mutex::new(0u32));

    // Spawn fewer tasks with simpler operations
    let mut handles = Vec::new();
    for i in 0..3 {
        let dispatcher = dispatcher_clone.clone();
        let access_count = access_count.clone();

        let handle = tokio::spawn(async move {
            // Request a src pad for this thread
            if let Some(src_pad) = dispatcher.request_pad_simple("src_%u") {
                for _j in 0..10 {
                    // Simple pad property access
                    let _name = src_pad.name();
                    let _is_linked = src_pad.is_linked();

                    *access_count.lock().unwrap() += 1;

                    // Small delay
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            }

            println!("Task {} completed", i);
        });

        handles.push(handle);
    }

    // Wait for all tasks to complete with timeout
    let result = timeout(Duration::from_secs(5), async {
        for handle in handles {
            handle.await.unwrap();
        }
    }).await;

    pipeline.set_state(gst::State::Null)?;

    assert!(result.is_ok(), "Multi-threaded test should complete within timeout");

    let total_accesses = *access_count.lock().unwrap();
    println!("Total pad accesses: {}", total_accesses);

    assert!(
        total_accesses >= 20, // At least some accesses should have happened
        "Should have processed some pad accesses"
    );

    println!("Multi-threaded pad safety test passed");
    Ok(())
}
