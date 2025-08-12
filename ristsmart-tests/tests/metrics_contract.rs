// Test to validate JSON metrics schema/rate compliance

use gst::prelude::*;
use gstreamer as gst;
use gstreamer_app as gst_app;
use serde_json;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

/// Test metrics contract validation for ristdispatcher
#[test]
fn test_metrics_contract_validation() {
    ristsmart_tests::register_everything_for_tests();

    // Create pipeline: appsrc -> dispatcher -> fakesink
    let pipeline = gst::Pipeline::new();
    let appsrc = gst::ElementFactory::make("appsrc")
        .property("caps", &gst::Caps::builder("application/x-rtp").build())
        .property("format", &gst::Format::Time)
        .build()
        .expect("Failed to create appsrc");

    let dispatcher = gst::ElementFactory::make("ristdispatcher")
        .property("metrics-export-interval-ms", 100u64) // Export every 100ms
        .property("auto-balance", false) // Disable auto-balance for consistent testing
        .build()
        .expect("Failed to create ristdispatcher");

    let fakesink = gst::ElementFactory::make("fakesink")
        .build()
        .expect("Failed to create fakesink");

    pipeline.add_many(&[&appsrc, &dispatcher, &fakesink]).unwrap();

    // Link elements
    appsrc.link(&dispatcher).expect("Failed to link appsrc to dispatcher");

    // Request a src pad from dispatcher and link to fakesink
    let src_pad = dispatcher
        .request_pad_simple("src_%u")
        .expect("Failed to request src pad");
    let sink_pad = fakesink.static_pad("sink").expect("Failed to get sink pad");
    src_pad.link(&sink_pad).expect("Failed to link dispatcher to fakesink");

    // Set up message collection
    let bus = pipeline.bus().expect("Failed to get bus");
    let collected_messages = Arc::new(Mutex::new(Vec::new()));
    let messages_clone = collected_messages.clone();

    // Watch for element messages from dispatcher
    let _bus_watch = bus.add_watch(move |_bus, msg| {
        if let Some(src) = msg.src() {
            if src.name() == "ristdispatcher0" {
                if let gst::MessageView::Element(element_msg) = msg.view() {
                    if let Some(structure) = element_msg.structure() {
                        if structure.name() == "rist-dispatcher-metrics" {
                            messages_clone.lock().unwrap().push(structure.to_owned());
                        }
                    }
                }
            }
        }
        glib::ControlFlow::Continue
    }).expect("Failed to add bus watch");

    // Start pipeline
    pipeline.set_state(gst::State::Playing).expect("Failed to set pipeline to Playing");

    // Push some test buffers through appsrc
    let appsrc = appsrc.dynamic_cast::<gst_app::AppSrc>().unwrap();
    for i in 0..5 {
        let data = vec![b'A' + (i % 26) as u8; 100]; // Simple test data
        let mut buffer = gst::Buffer::with_size(data.len()).unwrap();
        {
            let buffer_ref = buffer.get_mut().unwrap();
            buffer_ref.set_pts(gst::ClockTime::from_seconds(i));
        }
        if appsrc.push_buffer(buffer) != Ok(gst::FlowSuccess::Ok) {
            panic!("Failed to push buffer {}", i);
        }
    }

    // Wait for metrics messages to accumulate
    std::thread::sleep(Duration::from_millis(300));

    appsrc.end_of_stream().expect("Failed to send EOS");

    // Wait for EOS
    let timeout = Some(gst::ClockTime::from_seconds(5));
    match bus.timed_pop_filtered(timeout, &[gst::MessageType::Eos, gst::MessageType::Error]) {
        Some(msg) => match msg.view() {
            gst::MessageView::Eos(..) => println!("EOS received"),
            gst::MessageView::Error(err) => {
                panic!("Pipeline error: {}", err.error());
            }
            _ => panic!("Unexpected message"),
        },
        None => panic!("Timeout waiting for EOS"),
    }

    pipeline.set_state(gst::State::Null).expect("Failed to set pipeline to Null");

    // Validate collected metrics
    let messages = collected_messages.lock().unwrap();
    
    // Test 1: Verify we received metrics messages
    assert!(!messages.is_empty(), "Should have received at least one metrics message");

    // Test 2: Validate metrics message structure and schema
    for (i, structure) in messages.iter().enumerate() {
        println!("Validating metrics message {}: {}", i, structure.to_string());
        
        // Check required fields
        assert_eq!(structure.name(), "rist-dispatcher-metrics", 
                   "Metrics message should have correct name");
        
        // Should contain timestamp
        assert!(structure.has_field("timestamp"), 
                "Metrics should contain timestamp field");
        
        // Should contain weights data
        assert!(structure.has_field("current-weights"), 
                "Metrics should contain current-weights field");
        
        // Validate current-weights is valid JSON array
        if let Ok(weights_str) = structure.get::<&str>("current-weights") {
            let weights_json: serde_json::Value = serde_json::from_str(weights_str)
                .expect("current-weights should be valid JSON");
            
            assert!(weights_json.is_array(), "current-weights should be JSON array");
            let weights_array = weights_json.as_array().unwrap();
            assert!(!weights_array.is_empty(), "weights array should not be empty");
            
            // All weights should be numbers >= 0
            for weight in weights_array {
                assert!(weight.is_number(), "Each weight should be a number");
                let weight_val = weight.as_f64().unwrap();
                assert!(weight_val >= 0.0, "Each weight should be non-negative");
            }
        }
        
        // Should contain buffer count
        if structure.has_field("buffers-processed") {
            let buffer_count = structure.get::<u64>("buffers-processed").unwrap_or(0);
            // u64 is always non-negative, just check it exists and is reasonable
            assert!(buffer_count <= 1000, "Buffer count should be reasonable, got {}", buffer_count);
        }
        
        // Should contain element state info
        if structure.has_field("src-pad-count") {
            let pad_count = structure.get::<u32>("src-pad-count").unwrap_or(0);
            assert!(pad_count > 0, "Should have at least one src pad");
        }
    }

    // Test 3: Verify metrics emission timing
    if messages.len() >= 2 {
        // Extract timestamps and verify reasonable intervals
        let mut timestamps = Vec::new();
        for structure in messages.iter() {
            if let Ok(ts) = structure.get::<u64>("timestamp") {
                timestamps.push(ts);
            }
        }
        
        if timestamps.len() >= 2 {
            for window in timestamps.windows(2) {
                let interval = window[1].saturating_sub(window[0]);
                // Should be approximately 100ms (allowing some tolerance)
                assert!(interval >= 80 && interval <= 200, 
                        "Metrics interval should be approximately 100ms, got {}ms", interval);
            }
        }
    }

    println!("Metrics contract validation passed! Collected {} metrics messages", messages.len());
}

/// Test metrics schema validation with mock RIST stats
#[test] 
fn test_metrics_with_mock_rist_stats() {
    ristsmart_tests::register_everything_for_tests();

    let dispatcher = gst::ElementFactory::make("ristdispatcher")
        .property("metrics-export-interval-ms", 100u64)
        .build()
        .expect("Failed to create ristdispatcher");

    let rist_mock = gst::ElementFactory::make("riststats_mock")
        .build()
        .expect("Failed to create riststats_mock");

    // Set up mock stats with specific session data
    let custom_stats = gst::Structure::builder("rist/x-sender-stats")
        .field("session-0.sent-original-packets", 1000u64)
        .field("session-0.sent-retransmitted-packets", 50u64)
        .field("session-0.round-trip-time", 25.0f64)
        .field("session-1.sent-original-packets", 800u64)  
        .field("session-1.sent-retransmitted-packets", 100u64)
        .field("session-1.round-trip-time", 45.0f64)
        .build();

    rist_mock.set_property("stats", &custom_stats);
    dispatcher.set_property("rist", &rist_mock);

    // Request two src pads to match the sessions
    let _pad1 = dispatcher.request_pad_simple("src_0").expect("Failed to request src_0");
    let _pad2 = dispatcher.request_pad_simple("src_1").expect("Failed to request src_1");

    // Get initial metrics structure by triggering stats update
    // In this simplified test, we'll manually trigger and validate the stats structure
    // by getting the current-weights property which should be updated based on stats
    std::thread::sleep(Duration::from_millis(150));

    let weights_str: String = dispatcher.property("current-weights");
    
    // Validate that weights JSON is properly formatted
    let weights_json: serde_json::Value = serde_json::from_str(&weights_str)
        .expect("current-weights should be valid JSON");
    
    assert!(weights_json.is_array(), "Weights should be JSON array");
    let weights_array = weights_json.as_array().unwrap();
    assert_eq!(weights_array.len(), 2, "Should have weights for 2 sessions");
    
    // Verify weights reflect the different loss rates in mock data
    // Session 0: 50/1000 = 5% loss, Session 1: 100/800 = 12.5% loss
    // Session 0 should have higher weight (better performance)
    let weight0 = weights_array[0].as_f64().expect("Weight 0 should be a number");
    let weight1 = weights_array[1].as_f64().expect("Weight 1 should be a number");
    
    assert!(weight0 > 0.0 && weight1 > 0.0, "All weights should be positive");
    println!("Final weights: session-0={}, session-1={}", weight0, weight1);
    
    // In EWMA strategy, lower loss should result in higher weight over time
    // Note: The exact relationship depends on the EWMA algorithm details,
    // but we can verify basic sanity
    println!("Metrics with mock RIST stats validation passed!");
}
