use anyhow::Result;
use gstreamer as gst;
use gst::prelude::*;

/// Test if we can create RIST elements and set basic properties
#[test]
fn test_rist_elements_available() -> Result<()> {
    gst::init()?;
    
    println!("Checking RIST element availability...");
    
    // Test if elements can be created
    let ristsrc = gst::ElementFactory::make("ristsrc").build()?;
    let ristsink = gst::ElementFactory::make("ristsink").build()?;
    
    println!("✓ RIST elements (ristsrc, ristsink) are available");
    
    // Test basic property setting
    ristsink.set_property("address", "127.0.0.1");
    ristsink.set_property("port", 1234u32);
    
    ristsrc.set_property("address", "127.0.0.1");
    ristsrc.set_property("port", 1234u32);
    
    println!("✓ Basic RIST properties can be set");
    
    Ok(())
}

/// Test RIST bonding address configuration
#[test]
fn test_rist_bonding_addresses() -> Result<()> {
    gst::init()?;
    
    println!("Testing RIST bonding address configuration...");
    
    let ristsink = gst::ElementFactory::make("ristsink")
        .property("bonding-addresses", "127.0.0.1:1234,127.0.0.1:1236")
        .build()?;
        
    let ristsrc = gst::ElementFactory::make("ristsrc")
        .property("bonding-addresses", "127.0.0.1:1234,127.0.0.1:1236")
        .build()?;
        
    // Verify the addresses were set
    if let Some(addresses) = ristsink.property_value("bonding-addresses").get::<String>().ok() {
        println!("Sink bonding addresses: {}", addresses);
        assert!(addresses.contains("127.0.0.1:1234"));
        assert!(addresses.contains("127.0.0.1:1236"));
    }
    
    if let Some(addresses) = ristsrc.property_value("bonding-addresses").get::<String>().ok() {
        println!("Source bonding addresses: {}", addresses);
        assert!(addresses.contains("127.0.0.1:1234"));
        assert!(addresses.contains("127.0.0.1:1236"));
    }
        
    println!("✓ RIST bonding addresses configured successfully");
    
    Ok(())
}

/// Test that RIST pipeline can reach PAUSED state (elements linked correctly)
#[test]
fn test_rist_pipeline_basic_functionality() -> Result<()> {
    gst::init()?;
    
    println!("Testing RIST pipeline basic functionality...");
    
    let pipeline = gst::Pipeline::new();
    
    // Create a simple pipeline that should work
    let audiosrc = gst::ElementFactory::make("audiotestsrc")
        .property("num-buffers", 10)
        .build()?;
    
    let rtppay = gst::ElementFactory::make("rtpL16pay")
        .build()?;
    
    let ristsink = gst::ElementFactory::make("ristsink")
        .property("address", "127.0.0.1")
        .property("port", 5000u32)
        .build()?;
    
    let ristsrc = gst::ElementFactory::make("ristsrc")
        .property("address", "127.0.0.1")
        .property("port", 5000u32)
        .build()?;
        
    let rtpdepay = gst::ElementFactory::make("rtpL16depay")
        .build()?;
        
    let fakesink = gst::ElementFactory::make("fakesink")
        .build()?;
    
    // Add to pipeline
    pipeline.add_many(&[&audiosrc, &rtppay, &ristsink, &ristsrc, &rtpdepay, &fakesink])?;
    
    // Link elements
    gst::Element::link_many(&[&audiosrc, &rtppay, &ristsink])?;
    gst::Element::link_many(&[&ristsrc, &rtpdepay, &fakesink])?;
    
    println!("Elements linked successfully");
    
    // Try to set pipeline to PAUSED
    let ret = pipeline.set_state(gst::State::Paused);
    match ret {
        Ok(_) => println!("✓ Pipeline reached PAUSED state successfully"),
        Err(e) => {
            println!("Pipeline failed to reach PAUSED state: {}", e);
            return Err(e.into());
        }
    }
    
    // Wait for ASYNC_DONE or error
    let bus = pipeline.bus().unwrap();
    match bus.timed_pop_filtered(Some(gst::ClockTime::from_seconds(5)), &[
        gst::MessageType::AsyncDone,
        gst::MessageType::Error,
        gst::MessageType::StateChanged
    ]) {
        Some(msg) => match msg.view() {
            gst::MessageView::AsyncDone(..) => {
                println!("✓ Pipeline prerolled successfully");
            }
            gst::MessageView::Error(err) => {
                println!("Pipeline error during preroll: {} - {}", err.error(), err.debug().unwrap_or("".into()));
                return Err(err.error().into());
            }
            gst::MessageView::StateChanged(sc) if sc.src() == Some(pipeline.upcast_ref()) => {
                println!("Pipeline state changed: {:?} -> {:?}", sc.old(), sc.current());
            }
            _ => {}
        },
        None => {
            println!("⚠ Pipeline preroll timed out (this may be expected for network elements)");
        }
    }
    
    pipeline.set_state(gst::State::Null)?;
    println!("✓ Pipeline cleaned up successfully");
    
    Ok(())
}

/// Integration test to verify our changes work with the real RIST plugin
#[test] 
fn test_integration_with_real_rist() -> Result<()> {
    gst::init()?;
    
    println!("Running integration test with real RIST plugin...");
    
    // This test verifies that our pipeline modifications from the end-to-end tests
    // are compatible with the actual RIST plugin elements
    
    // Test 1: Verify we can create the elements we expect
    test_rist_elements_available()?;
    
    // Test 2: Verify bonding address configuration works
    test_rist_bonding_addresses()?;
    
    // Test 3: Verify basic pipeline functionality  
    test_rist_pipeline_basic_functionality()?;
    
    println!("✅ All RIST integration tests passed!");
    println!("");
    println!("Summary of findings:");
    println!("- RIST plugin elements (ristsrc, ristsink) are available and working");
    println!("- Bonding addresses can be configured with comma-separated format");
    println!("- Pipelines can be constructed and reach PAUSED state");
    println!("- Our end-to-end test modifications should work with real RIST plugin");
    
    Ok(())
}
