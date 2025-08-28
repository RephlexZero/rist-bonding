use gst::prelude::*;
use gstreamer as gst;
use gstristelements::testing::*;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Pump the GLib main loop for the specified duration
fn run_mainloop_ms(ms: u64) {
    // Pump the default GLib main context where timeout_add registered
    let ctx = glib::MainContext::default();
    let _guard = ctx.acquire().expect("acquire main context");
    let end = std::time::Instant::now() + std::time::Duration::from_millis(ms);
    while std::time::Instant::now() < end {
        // Drain all pending events without blocking, then sleep briefly.
        while ctx.iteration(false) {}
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
fn test_metrics_debug() {
    init_for_tests();
    println!("=== Metrics Debug Test ===");
    
    // Create minimal pipeline  
    let pipeline = gst::Pipeline::new();
    let dispatcher = create_dispatcher_for_testing(Some(&[1.0])); // Just one output
    
    // Try enabling auto-balance to see if it affects metrics
    dispatcher.set_property("auto-balance", true);
    println!("Enabled auto-balance on dispatcher");
    
    let source = create_test_source();
    let sink = create_counter_sink();
    
    pipeline.add_many([&source, &dispatcher, &sink]).unwrap();
    
    // Link elements
    source.link(&dispatcher).unwrap();
    let src_pad = dispatcher.request_pad_simple("src_%u").unwrap();
    src_pad.link(&sink.static_pad("sink").unwrap()).unwrap();
    
    println!("Pipeline linked successfully");
    
    // Check dispatcher state before starting
    println!("Dispatcher name: {}", dispatcher.name());
    println!("Dispatcher factory: {}", dispatcher.factory().unwrap().name());
    
    if let Some(sinkpad) = dispatcher.static_pad("sink") {
        println!("Has sink pad: {}", sinkpad.name());
    } else {
        println!("No sink pad found");
    }
    
    // Set up bus watch on BOTH pipeline and element buses
    let pipeline_bus = pipeline.bus().unwrap();
    let element_bus = dispatcher.bus();
    
    let messages = Arc::new(Mutex::new(Vec::<String>::new()));
    let msg_clone1 = messages.clone();
    let msg_clone2 = messages.clone();
    
    // Watch pipeline bus
    let _watch1 = pipeline_bus.add_watch(move |_bus, message| {
        match message.type_() {
            gst::MessageType::Application => {
                if let Some(structure) = message.structure() {
                    if structure.name() == "rist-dispatcher-metrics" {
                        println!("🎉 Received metrics message on PIPELINE bus!");
                        msg_clone1.lock().unwrap().push(format!("pipeline: {}", structure.to_string()));
                    }
                }
            }
            _ => {}
        }
        glib::ControlFlow::Continue
    });
    
    // Watch element bus if it exists  
    let _watch2 = if let Some(elem_bus) = element_bus {
        println!("Also watching element bus");
        Some(elem_bus.add_watch(move |_bus, message| {
            match message.type_() {
                gst::MessageType::Application => {
                    if let Some(structure) = message.structure() {
                        if structure.name() == "rist-dispatcher-metrics" {
                            println!("🎉 Received metrics message on ELEMENT bus!");
                            msg_clone2.lock().unwrap().push(format!("element: {}", structure.to_string()));
                        }
                    }
                }
                _ => {}
            }
            glib::ControlFlow::Continue
        }))
    } else {
        println!("No element bus available");
        None
    };
    
    // Start pipeline
    println!("Starting pipeline...");
    pipeline.set_state(gst::State::Playing).unwrap();
    
    // Wait a moment
    std::thread::sleep(Duration::from_millis(200));
    
    // Set metrics property
    println!("Setting metrics interval to 300ms...");
    dispatcher.set_property("metrics-export-interval-ms", 300u64);
    
    let interval: u64 = dispatcher.property("metrics-export-interval-ms");
    println!("Confirmed interval: {}ms", interval);
    
    // Wait for metrics and PUMP THE MAIN LOOP (this is the key!)
    println!("Waiting 1 second for metrics while pumping main loop...");
    run_mainloop_ms(1000);
    
    // Check results
    let msg_count = messages.lock().unwrap().len();
    println!("Received {} metrics messages", msg_count);
    
    // Stop pipeline
    pipeline.set_state(gst::State::Null).unwrap();
    
    assert!(msg_count > 0, "Should have received at least one metrics message");
    println!("✅ Debug test passed");
}