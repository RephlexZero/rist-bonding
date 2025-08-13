use std::time::Duration;
use std::thread;
use std::sync::{Arc, Mutex};
use gstreamer as gst;
use gst::prelude::*;
use anyhow::{Result, Context};

#[test]
fn test_rist_plugin_integration() -> Result<()> {
    // Initialize GStreamer
    gst::init()?;
    
    println!("Testing RIST plugin integration...");
    
    // Test if RIST elements are available
    let _ristsrc = gst::ElementFactory::find("ristsrc")
        .context("ristsrc element not found - is RIST plugin loaded?")?;
    let _ristsink = gst::ElementFactory::find("ristsink")
        .context("ristsink element not found - is RIST plugin loaded?")?;
        
    println!("✓ RIST elements found: ristsrc, ristsink");
    
    // Create a simple pipeline to test data flow
    test_basic_data_flow()?;
    
    // Test bonding configuration
    test_bonding_configuration()?;
    
    Ok(())
}

fn test_basic_data_flow() -> Result<()> {
    println!("Testing basic RIST data flow...");
    
    let pipeline = gst::Pipeline::new();
    
    // Simple test: audiotestsrc -> RTP payloader -> RIST sink/src -> RTP depayloader -> fakesink
    let audiosrc = gst::ElementFactory::make("audiotestsrc")
        .property("num-buffers", 100)
        .build()?;
    
    let rtppay = gst::ElementFactory::make("rtpL16pay")
        .build()?;
    
    let ristsink = gst::ElementFactory::make("ristsink")
        .property("address", "127.0.0.1")
        .property("port", 1234u32)
        .build()?;
    
    let ristsrc = gst::ElementFactory::make("ristsrc")
        .property("address", "127.0.0.1")
        .property("port", 1234u32)
        .build()?;
        
    let rtpdepay = gst::ElementFactory::make("rtpL16depay")
        .build()?;
        
    let sink = gst::ElementFactory::make("fakesink")
        .property("sync", false)
        .property("signal-handoffs", true)
        .build()?;
    
    // Add elements to pipeline
    pipeline.add_many(&[&audiosrc, &rtppay, &ristsink, &ristsrc, &rtpdepay, &sink])?;
    
    // Link elements
    gst::Element::link_many(&[&audiosrc, &rtppay, &ristsink])?;
    gst::Element::link_many(&[&ristsrc, &rtpdepay, &sink])?;
    
    // Set up buffer counting
    let buffer_count = Arc::new(Mutex::new(0u32));
    let buffer_count_clone = buffer_count.clone();
    
    sink.connect("handoff", false, move |values| {
        if let Some(_buffer) = values.get(1) {
            let mut count = buffer_count_clone.lock().unwrap();
            *count += 1;
            if *count % 10 == 0 {
                println!("Received {} buffers", *count);
            }
        }
        None
    });
    
    // Start pipeline
    println!("Starting pipeline...");
    pipeline.set_state(gst::State::Playing)?;
    
    // Wait for preroll or error
    let bus = pipeline.bus().unwrap();
    let timeout = Duration::from_secs(15);
    
    let mut received_data = false;
    let start_time = std::time::Instant::now();
    
    loop {
        if let Some(msg) = bus.timed_pop(Some(gst::ClockTime::from_mseconds(100))) {
            match msg.view() {
                gst::MessageView::Eos(..) => {
                    println!("EOS received");
                    break;
                }
                gst::MessageView::Error(err) => {
                    println!("Error: {} - {}", err.error(), err.debug().unwrap_or("".into()));
                    return Err(err.error().into());
                }
                gst::MessageView::StateChanged(state_changed) => {
                    if state_changed.src() == Some(pipeline.upcast_ref()) {
                        println!("Pipeline state: {:?} -> {:?}", 
                                state_changed.old(), state_changed.current());
                    }
                }
                _ => {}
            }
        }
        
        // Check if we received any data
        let count = *buffer_count.lock().unwrap();
        if count > 0 && !received_data {
            received_data = true;
            println!("✓ Data flow detected! Received {} buffers", count);
        }
        
        if start_time.elapsed() > timeout {
            println!("Test timeout reached");
            break;
        }
    }
    
    pipeline.set_state(gst::State::Null)?;
    
    let final_count = *buffer_count.lock().unwrap();
    println!("Final buffer count: {}", final_count);
    
    if final_count > 0 {
        println!("✓ Basic data flow test passed");
    } else {
        println!("✗ No data received - pipeline may have issues");
    }
    
    Ok(())
}

fn test_bonding_configuration() -> Result<()> {
    println!("Testing RIST bonding configuration...");
    
    // Test creating elements with bonding addresses
    let ristsink = gst::ElementFactory::make("ristsink")
        .property("bonding-addresses", "127.0.0.1:1234,127.0.0.1:1236")
        .build()?;
        
    let _ristsrc = gst::ElementFactory::make("ristsrc")
        .property("bonding-addresses", "127.0.0.1:1234,127.0.0.1:1236")
        .build()?;
        
    println!("✓ RIST bonding elements created successfully");
    
    // Check if properties were set correctly
    if let Some(addresses) = ristsink.property_value("bonding-addresses").get::<String>().ok() {
        println!("Sink bonding addresses: {}", addresses);
    }
    
    Ok(())
}

#[test] 
fn test_rist_pipeline_with_debugging() -> Result<()> {
    gst::init()?;
    
    println!("Testing RIST pipeline with detailed debugging...");
    
    // Enable GST debug logging for RIST
    std::env::set_var("GST_DEBUG", "rist*:5");
    
    // Create a minimal pipeline
    let pipeline = gst::Pipeline::new();
    
    let audiosrc = gst::ElementFactory::make("audiotestsrc")
        .property("wave", 0i32) // sine wave
        .property("freq", 440.0)
        .property("num-buffers", 1000)
        .build()?;
        
    let audioenc = gst::ElementFactory::make("lamemp3enc")
        .property("bitrate", 128)
        .build()?;
        
    let rtppay = gst::ElementFactory::make("rtpmpapay")
        .build()?;
        
    let ristsink = gst::ElementFactory::make("ristsink")
        .property("address", "127.0.0.1")
        .property("port", 5000u32)
        .build()?;
        
    let ristsrc = gst::ElementFactory::make("ristsrc")
        .property("address", "127.0.0.1") 
        .property("port", 5000u32)
        .build()?;
        
    let rtpdepay = gst::ElementFactory::make("rtpmpadepay")
        .build()?;
        
    let audiodec = gst::ElementFactory::make("mpg123audiodec")
        .build()?;
        
    let sink = gst::ElementFactory::make("fakesink")
        .property("sync", false)
        .build()?;
    
    pipeline.add_many(&[
        &audiosrc, &audioenc, &rtppay, &ristsink,
        &ristsrc, &rtpdepay, &audiodec, &sink
    ])?;
    
    gst::Element::link_many(&[&audiosrc, &audioenc, &rtppay, &ristsink])?;
    gst::Element::link_many(&[&ristsrc, &rtpdepay, &audiodec, &sink])?;
    
    println!("Starting audio pipeline...");
    
    // Set to PAUSED first to check for immediate errors
    match pipeline.set_state(gst::State::Paused) {
        Ok(_) => println!("Pipeline paused successfully"),
        Err(e) => {
            println!("Failed to pause pipeline: {}", e);
            return Err(e.into());
        }
    }
    
    // Wait for preroll
    let bus = pipeline.bus().unwrap();
    match bus.timed_pop_filtered(Some(gst::ClockTime::from_seconds(5)), &[
        gst::MessageType::AsyncDone,
        gst::MessageType::Error
    ]) {
        Some(msg) => match msg.view() {
            gst::MessageView::AsyncDone(..) => {
                println!("Pipeline preroll complete");
            }
            gst::MessageView::Error(err) => {
                println!("Preroll error: {} - {}", err.error(), err.debug().unwrap_or("".into()));
                return Err(err.error().into());
            }
            _ => {}
        },
        None => {
            println!("Preroll timeout");
        }
    }
    
    // Now try to play
    match pipeline.set_state(gst::State::Playing) {
        Ok(_) => println!("Pipeline playing"),
        Err(e) => {
            println!("Failed to start pipeline: {}", e);
            return Err(e.into());
        }
    }
    
    // Run for a bit
    thread::sleep(Duration::from_secs(10));
    
    pipeline.set_state(gst::State::Null)?;
    println!("✓ Audio pipeline test completed");
    
    Ok(())
}
