use gst::prelude::*;
use gstreamer as gst;
use std::time::Duration;

fn main() {
    // Initialize GStreamer
    gst::init().unwrap();
    
    // Register our plugins
    gstristelements::plugin_init().unwrap();
    
    // Create pipeline
    let pipeline = gst::Pipeline::new();
    
    // Create dispatcher
    let dispatcher = gst::ElementFactory::make("ristdispatcher")
        .property("weights", "[0.5, 0.5]")
        .build()
        .unwrap();
    
    println!("Created dispatcher: {:?}", dispatcher.name());
    
    // Add to pipeline (important for bus access)
    pipeline.add(&dispatcher).unwrap();
    
    // Set metrics property AFTER adding to pipeline
    dispatcher.set_property("metrics-export-interval-ms", 500u64);
    let interval: u64 = dispatcher.property("metrics-export-interval-ms");
    println!("Set metrics interval to: {}ms", interval);
    
    // Set up bus watch
    let bus = pipeline.bus().unwrap();
    let mut message_count = 0;
    
    let _watch_id = bus.add_watch(move |_bus, message| {
        match message.type_() {
            gst::MessageType::Application => {
                if let Some(structure) = message.structure() {
                    if structure.name() == "rist-dispatcher-metrics" {
                        message_count += 1;
                        println!("Received metrics message #{}: {}", message_count, structure.to_string());
                    }
                }
            }
            _ => {}
        }
        glib::ControlFlow::Continue
    });
    
    // Start pipeline
    println!("Starting pipeline...");
    pipeline.set_state(gst::State::Playing).unwrap();
    
    // Wait for messages
    println!("Waiting 2 seconds for metrics messages...");
    std::thread::sleep(Duration::from_secs(2));
    
    // Stop pipeline
    pipeline.set_state(gst::State::Null).unwrap();
    
    println!("Test completed.");
}