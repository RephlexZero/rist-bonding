use gstreamer::prelude::*;
use gstristelements::testing::*;
use std::time::Duration;

#[test]
fn test_dynbitrate_pipeline_state_transitions() {
    init_for_tests();
    
    let pipeline = gstreamer::Pipeline::new();
    let dynbitrate = create_dynbitrate();
    let encoder_stub = create_encoder_stub(Some(2000)); // 2000 kbps
    let riststats_mock = create_riststats_mock(Some(0.85), Some(40));
    
    // Configure dynbitrate
    dynbitrate.set_property("encoder", &encoder_stub);
    dynbitrate.set_property("rist", &riststats_mock);
    dynbitrate.set_property("min-kbps", 1000u32);
    dynbitrate.set_property("max-kbps", 4000u32);
    
    pipeline.add_many([&dynbitrate, &encoder_stub, &riststats_mock]).unwrap();
    
    // Test state transitions: NULL -> READY -> PAUSED -> PLAYING -> PAUSED -> NULL
    println!("Testing NULL -> READY transition");
    pipeline.set_state(gstreamer::State::Ready).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    
    println!("Testing READY -> PAUSED transition");  
    pipeline.set_state(gstreamer::State::Paused).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    
    println!("Testing PAUSED -> PLAYING transition");
    pipeline.set_state(gstreamer::State::Playing).unwrap();
    std::thread::sleep(Duration::from_millis(200));
    
    println!("Testing PLAYING -> PAUSED transition");
    pipeline.set_state(gstreamer::State::Paused).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    
    println!("Testing PAUSED -> READY transition");
    pipeline.set_state(gstreamer::State::Ready).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    
    println!("Testing READY -> NULL transition");
    pipeline.set_state(gstreamer::State::Null).unwrap();
    
    println!("dynbitrate state transitions test passed");
}

#[test]
fn test_dynbitrate_pause_resume_behavior() {
    init_for_tests();
    
    let pipeline = gstreamer::Pipeline::new();
    let dynbitrate = create_dynbitrate();
    let encoder_stub = create_encoder_stub(Some(1500)); // 1500 kbps
    let riststats_mock = create_riststats_mock(Some(0.70), Some(100)); // Poor conditions
    
    dynbitrate.set_property("encoder", &encoder_stub);
    dynbitrate.set_property("rist", &riststats_mock);
    dynbitrate.set_property("min-kbps", 800u32);
    dynbitrate.set_property("max-kbps", 3000u32);
    dynbitrate.set_property("step-kbps", 200u32);
    
    pipeline.add_many([&dynbitrate, &encoder_stub, &riststats_mock]).unwrap();
    
    // Start playing
    pipeline.set_state(gstreamer::State::Playing).unwrap();
    std::thread::sleep(Duration::from_millis(150));
    
    // Pause and verify element handles it gracefully
    println!("Pausing pipeline");
    pipeline.set_state(gstreamer::State::Paused).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    
    // Resume playing 
    println!("Resuming pipeline");
    pipeline.set_state(gstreamer::State::Playing).unwrap();
    std::thread::sleep(Duration::from_millis(150));
    
    // Another pause/resume cycle
    println!("Second pause");
    pipeline.set_state(gstreamer::State::Paused).unwrap();
    std::thread::sleep(Duration::from_millis(80));
    
    println!("Second resume");
    pipeline.set_state(gstreamer::State::Playing).unwrap();
    std::thread::sleep(Duration::from_millis(120));
    
    pipeline.set_state(gstreamer::State::Null).unwrap();
    
    println!("dynbitrate pause/resume behavior test passed");
}

#[test]
fn test_dynbitrate_element_lifecycle() {
    init_for_tests();
    
    // Test multiple create/destroy cycles
    for cycle in 0..3 {
        println!("Element lifecycle cycle {}", cycle + 1);
        
        let dynbitrate = create_dynbitrate();
        let encoder_stub = create_encoder_stub(Some(2500));
        let riststats_mock = create_riststats_mock(Some(0.90), Some(25));
        
        // Configure element
        dynbitrate.set_property("encoder", &encoder_stub);
        dynbitrate.set_property("rist", &riststats_mock);
        dynbitrate.set_property("min-kbps", 1200u32);
        dynbitrate.set_property("max-kbps", 5000u32);
        dynbitrate.set_property("downscale-keyunit", cycle % 2 == 0); // Alternate setting
        
        let pipeline = gstreamer::Pipeline::new();
        pipeline.add_many([&dynbitrate, &encoder_stub, &riststats_mock]).unwrap();
        
        // Brief lifecycle test
        pipeline.set_state(gstreamer::State::Playing).unwrap();
        std::thread::sleep(Duration::from_millis(100));
        pipeline.set_state(gstreamer::State::Null).unwrap();
        
        // Elements should clean up properly when pipeline is destroyed
        drop(pipeline);
        
        println!("  Cycle {} completed", cycle + 1);
    }
    
    println!("dynbitrate element lifecycle test passed");
}

#[test]
fn test_dynbitrate_rapid_state_changes() {
    init_for_tests();
    
    let pipeline = gstreamer::Pipeline::new();
    let dynbitrate = create_dynbitrate();
    let encoder_stub = create_encoder_stub(Some(1800));
    let riststats_mock = create_riststats_mock(Some(0.75), Some(60));
    
    dynbitrate.set_property("encoder", &encoder_stub);
    dynbitrate.set_property("rist", &riststats_mock);
    dynbitrate.set_property("min-kbps", 600u32);
    dynbitrate.set_property("max-kbps", 3500u32);
    
    pipeline.add_many([&dynbitrate, &encoder_stub, &riststats_mock]).unwrap();
    
    // Test rapid state transitions
    let state_sequences = [
        (gstreamer::State::Ready, "READY"),
        (gstreamer::State::Paused, "PAUSED"),  
        (gstreamer::State::Playing, "PLAYING"),
        (gstreamer::State::Paused, "PAUSED"),
        (gstreamer::State::Ready, "READY"),
        (gstreamer::State::Playing, "PLAYING"), // Skip PAUSED
        (gstreamer::State::Null, "NULL"),
    ];
    
    for (state, state_name) in &state_sequences {
        println!("Rapidly transitioning to {}", state_name);
        pipeline.set_state(*state).unwrap();
        std::thread::sleep(Duration::from_millis(30)); // Very brief delays
    }
    
    println!("dynbitrate rapid state changes test passed");
}

#[test]
fn test_dispatcher_lifecycle_management() {
    init_for_tests();
    
    // Test dispatcher lifecycle through multiple configurations
    for iteration in 0..3 {
        println!("Dispatcher lifecycle iteration {}", iteration + 1);
        
        let dispatcher = create_dispatcher(Some(&[1.0, 1.0]));
        let counter1 = create_counter_sink();
        let counter2 = create_counter_sink();
        let source = create_test_source();
        
        // Configure dispatcher with different settings each iteration
        match iteration {
            0 => {
                dispatcher.set_property("strategy", "round-robin");
                dispatcher.set_property("auto-balance", false);
            }
            1 => {
                dispatcher.set_property("strategy", "weighted");
                dispatcher.set_property("auto-balance", true);
                dispatcher.set_property("min-hold-ms", 200u64);
            }
            2 => {
                dispatcher.set_property("strategy", "quality-weighted");
                dispatcher.set_property("auto-balance", true);
                dispatcher.set_property("switch-threshold", 1.5); // Valid range is 1.0-10.0
            }
            _ => {}
        }
        
        let pipeline = gstreamer::Pipeline::new();
        pipeline.add_many([&source, &dispatcher, &counter1, &counter2]).unwrap();
        
        // Link elements
        source.link(&dispatcher).unwrap();
        let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
        let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();
        
        src_0.link(&counter1.static_pad("sink").unwrap()).unwrap();
        src_1.link(&counter2.static_pad("sink").unwrap()).unwrap();
        
        // Test lifecycle
        dispatcher.set_state(gstreamer::State::Playing).unwrap();
        std::thread::sleep(Duration::from_millis(150));
        
        // Clean up pads and state
        dispatcher.release_request_pad(&src_0);
        dispatcher.release_request_pad(&src_1);
        dispatcher.set_state(gstreamer::State::Null).unwrap();
        
        drop(pipeline);
        
        println!("  Dispatcher iteration {} completed", iteration + 1);
    }
    
    println!("dispatcher lifecycle management test passed");
}

#[test] 
fn test_element_cleanup_and_resource_management() {
    init_for_tests();
    
    // Test resource management across multiple elements
    println!("Testing resource management and cleanup");
    
    let mut pipelines = Vec::new();
    
    // Create multiple pipeline instances
    for i in 0..3 {
        let pipeline = gstreamer::Pipeline::new();
        let dynbitrate = create_dynbitrate();
        let encoder_stub = create_encoder_stub(Some(1000 + (i * 500)));
        let riststats_mock = create_riststats_mock(Some(0.8 - (i as f64 * 0.1)), Some(50 + (i * 20)));
        
        dynbitrate.set_property("encoder", &encoder_stub);
        dynbitrate.set_property("rist", &riststats_mock);
        
        pipeline.add_many([&dynbitrate, &encoder_stub, &riststats_mock]).unwrap();
        
        // Start pipeline
        pipeline.set_state(gstreamer::State::Playing).unwrap();
        pipelines.push(pipeline);
    }
    
    // Let all pipelines run briefly
    std::thread::sleep(Duration::from_millis(200));
    
    // Clean up all pipelines
    for (index, pipeline) in pipelines.iter().enumerate() {
        println!("Cleaning up pipeline {}", index + 1);
        pipeline.set_state(gstreamer::State::Null).unwrap();
    }
    
    // Drop all pipelines to test cleanup
    drop(pipelines);
    
    println!("element cleanup and resource management test passed");
}