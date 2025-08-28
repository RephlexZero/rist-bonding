use gstreamer::prelude::*;
use gstristelements::testing::*;
use std::time::Duration;

#[test]
fn test_downscale_keyunit_property() {
    init_for_tests();
    
    let dynbitrate = create_dynbitrate();
    let encoder_stub = create_encoder_stub(Some(2000)); // 2000 kbps initial
    let riststats_mock = create_riststats_mock(Some(0.95), Some(30)); // Good quality
    
    // Configure dynbitrate elements
    dynbitrate.set_property("encoder", &encoder_stub);
    dynbitrate.set_property("rist", &riststats_mock);
    dynbitrate.set_property("min-kbps", 500u32);
    dynbitrate.set_property("max-kbps", 5000u32);
    dynbitrate.set_property("step-kbps", 250u32);
    
    // Test property default value (should be false)
    let default_keyunit: bool = dynbitrate.property("downscale-keyunit");
    assert!(!default_keyunit, "downscale-keyunit should default to false");
    
    // Test setting property to true
    dynbitrate.set_property("downscale-keyunit", true);
    let enabled_keyunit: bool = dynbitrate.property("downscale-keyunit");
    assert!(enabled_keyunit, "downscale-keyunit should be enabled after setting to true");
    
    // Test setting property to false
    dynbitrate.set_property("downscale-keyunit", false);
    let disabled_keyunit: bool = dynbitrate.property("downscale-keyunit");
    assert!(!disabled_keyunit, "downscale-keyunit should be disabled after setting to false");
    
    println!("downscale-keyunit property validation passed");
}

#[test] 
fn test_keyunit_configuration_scenarios() {
    init_for_tests();
    
    let scenarios = [
        (true, "enabled"),
        (false, "disabled"),
    ];
    
    for (keyunit_enabled, description) in &scenarios {
        println!("Testing keyunit configuration: {}", description);
        
        let dynbitrate = create_dynbitrate();
        let encoder_stub = create_encoder_stub(Some(1500)); // 1500 kbps
        let riststats_mock = create_riststats_mock(Some(0.90), Some(40)); // Good quality
        
        // Configure dynbitrate
        dynbitrate.set_property("encoder", &encoder_stub);
        dynbitrate.set_property("rist", &riststats_mock);
        dynbitrate.set_property("downscale-keyunit", *keyunit_enabled);
        
        // Verify the property was set correctly
        let actual_keyunit: bool = dynbitrate.property("downscale-keyunit");
        assert_eq!(actual_keyunit, *keyunit_enabled, 
                   "downscale-keyunit property should match configured value");
                   
        println!("  {} configuration validated", description);
    }
}

#[test]
fn test_dynbitrate_with_keyunit_enabled() {
    init_for_tests();
    
    let pipeline = gstreamer::Pipeline::new();
    let dynbitrate = create_dynbitrate();
    let encoder_stub = create_encoder_stub(Some(2500)); // 2500 kbps
    let riststats_mock = create_riststats_mock(Some(0.85), Some(50)); // Moderate quality
    
    // Configure dynbitrate with keyunit enabled
    dynbitrate.set_property("encoder", &encoder_stub);
    dynbitrate.set_property("rist", &riststats_mock);
    dynbitrate.set_property("min-kbps", 1000u32);
    dynbitrate.set_property("max-kbps", 4000u32);
    dynbitrate.set_property("step-kbps", 500u32);
    dynbitrate.set_property("target-loss-pct", 2.0);
    dynbitrate.set_property("downscale-keyunit", true);
    
    // Add elements to pipeline
    pipeline.add_many([&dynbitrate, &encoder_stub, &riststats_mock]).unwrap();
    
    // Set pipeline to playing state
    pipeline.set_state(gstreamer::State::Playing).unwrap();
    
    // Allow pipeline to initialize
    std::thread::sleep(Duration::from_millis(200));
    
    pipeline.set_state(gstreamer::State::Null).unwrap();
    
    println!("dynbitrate with keyunit enabled operational test passed");
}

#[test]
fn test_keyunit_property_validation() {
    init_for_tests();
    
    let dynbitrate = create_dynbitrate();
    let encoder_stub = create_encoder_stub(Some(1000)); // 1000 kbps
    let riststats_mock = create_riststats_mock(Some(0.88), Some(35)); // Good quality
    
    // Configure basic properties
    dynbitrate.set_property("encoder", &encoder_stub);
    dynbitrate.set_property("rist", &riststats_mock);
    
    // Test boolean property validation
    let test_values = [true, false, true, false];
    
    for (index, &value) in test_values.iter().enumerate() {
        dynbitrate.set_property("downscale-keyunit", value);
        let retrieved: bool = dynbitrate.property("downscale-keyunit");
        assert_eq!(retrieved, value, 
                   "Iteration {}: downscale-keyunit property should retain value {}", 
                   index, value);
    }
    
    println!("keyunit property validation test passed");
}