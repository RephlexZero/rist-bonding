use gstreamer::prelude::*;
use gstristelements::testing::*;
use std::time::Duration;

#[test]
fn test_dynbitrate_with_no_stats() {
    init_for_tests();
    
    let dynbitrate = create_dynbitrate();
    let encoder_stub = create_encoder_stub(Some(2000)); // 2000 kbps initial
    
    // Configure dynbitrate WITHOUT rist element (no stats)
    dynbitrate.set_property("encoder", &encoder_stub);
    // Note: NOT setting "rist" property - simulating no stats available
    dynbitrate.set_property("min-kbps", 1000u32);
    dynbitrate.set_property("max-kbps", 5000u32);
    dynbitrate.set_property("step-kbps", 250u32);
    dynbitrate.set_property("target-loss-pct", 1.0);
    
    let pipeline = gstreamer::Pipeline::new();
    pipeline.add_many([&dynbitrate, &encoder_stub]).unwrap();
    
    // Test that element handles no stats gracefully
    pipeline.set_state(gstreamer::State::Playing).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    
    // Element should continue operating even without stats
    // (may use default/safe behavior)
    pipeline.set_state(gstreamer::State::Null).unwrap();
    
    println!("dynbitrate with no stats test passed");
}

#[test]
fn test_dynbitrate_stats_unavailable_scenario() {
    init_for_tests();
    
    let dynbitrate = create_dynbitrate();
    let encoder_stub = create_encoder_stub(Some(1500)); // 1500 kbps initial
    
    // Create a stats mock that doesn't provide statistics initially
    let riststats_mock = create_riststats_mock(None, None); // No quality/RTT data
    
    // Configure dynbitrate with stats element but no data
    dynbitrate.set_property("encoder", &encoder_stub);
    dynbitrate.set_property("rist", &riststats_mock);
    dynbitrate.set_property("min-kbps", 800u32);
    dynbitrate.set_property("max-kbps", 4000u32);
    dynbitrate.set_property("step-kbps", 200u32);
    
    let pipeline = gstreamer::Pipeline::new();
    pipeline.add_many([&dynbitrate, &encoder_stub, &riststats_mock]).unwrap();
    
    // Test graceful handling of unavailable statistics
    pipeline.set_state(gstreamer::State::Playing).unwrap();
    std::thread::sleep(Duration::from_millis(200));
    
    // Element should handle missing stats gracefully
    pipeline.set_state(gstreamer::State::Null).unwrap();
    
    println!("dynbitrate with unavailable stats test passed");
}

#[test]
fn test_dynbitrate_stats_quality_extremes() {
    init_for_tests();
    
    let scenarios = [
        (Some(0.0), Some(1000), "extremely poor quality"),
        (Some(1.0), Some(5), "perfect quality, very low RTT"),
        (Some(0.5), Some(2000), "moderate quality, high RTT"),
        (Some(0.99), Some(10), "near perfect quality, very low RTT"),
    ];
    
    for (quality, rtt, description) in &scenarios {
        println!("Testing dynbitrate with {}", description);
        
        let dynbitrate = create_dynbitrate();
        let encoder_stub = create_encoder_stub(Some(2000)); // 2000 kbps
        let riststats_mock = create_riststats_mock(*quality, *rtt);
        
        dynbitrate.set_property("encoder", &encoder_stub);
        dynbitrate.set_property("rist", &riststats_mock);
        dynbitrate.set_property("min-kbps", 500u32);
        dynbitrate.set_property("max-kbps", 6000u32);
        dynbitrate.set_property("step-kbps", 300u32);
        dynbitrate.set_property("target-loss-pct", 1.5);
        
        let pipeline = gstreamer::Pipeline::new();
        pipeline.add_many([&dynbitrate, &encoder_stub, &riststats_mock]).unwrap();
        
        // Test element behavior with extreme quality values
        pipeline.set_state(gstreamer::State::Playing).unwrap();
        std::thread::sleep(Duration::from_millis(250));
        
        // Element should handle extreme values appropriately
        pipeline.set_state(gstreamer::State::Null).unwrap();
        
        println!("  {} scenario completed", description);
    }
    
    println!("dynbitrate extreme quality scenarios test passed");
}

#[test]
fn test_dynbitrate_stats_rapid_changes() {
    init_for_tests();
    
    let dynbitrate = create_dynbitrate();
    let encoder_stub = create_encoder_stub(Some(3000)); // 3000 kbps initial
    
    // Test with rapidly changing stats simulation
    let scenarios = [
        (Some(0.9), Some(50), "good initial"),
        (Some(0.1), Some(200), "sudden degradation"), 
        (Some(0.95), Some(30), "quick recovery"),
        (Some(0.3), Some(500), "another drop"),
        (Some(0.85), Some(60), "gradual improvement"),
    ];
    
    for (quality, rtt, description) in &scenarios {
        println!("Simulating stats change: {}", description);
        
        let riststats_mock = create_riststats_mock(*quality, *rtt);
        
        // Update dynbitrate with new stats element (simulating changing conditions)
        dynbitrate.set_property("encoder", &encoder_stub);
        dynbitrate.set_property("rist", &riststats_mock);
        dynbitrate.set_property("min-kbps", 1000u32);
        dynbitrate.set_property("max-kbps", 5000u32);
        dynbitrate.set_property("step-kbps", 400u32);
        
        let pipeline = gstreamer::Pipeline::new();
        pipeline.add_many([&dynbitrate, &encoder_stub, &riststats_mock]).unwrap();
        
        // Brief test of each condition
        pipeline.set_state(gstreamer::State::Playing).unwrap();
        std::thread::sleep(Duration::from_millis(150));
        pipeline.set_state(gstreamer::State::Null).unwrap();
        
        println!("  {} condition tested", description);
    }
    
    println!("dynbitrate rapid stats changes test passed");
}

#[test]
fn test_dynbitrate_configuration_validation() {
    init_for_tests();
    
    // Test various property configurations
    let configs = [
        (100u32, 10000u32, 50u32, 0.1, "wide range, small steps"),
        (2000u32, 2500u32, 100u32, 3.0, "narrow range, precise steps"),
        (1000u32, 1000u32, 250u32, 1.0, "equal min/max"),
        (5000u32, 8000u32, 1000u32, 5.0, "high bitrate range"),
    ];
    
    for (min_kbps, max_kbps, step_kbps, target_loss, description) in &configs {
        println!("Testing configuration: {}", description);
        
        let dynbitrate = create_dynbitrate();
        let encoder_stub = create_encoder_stub(Some(*min_kbps + 500)); 
        let riststats_mock = create_riststats_mock(Some(0.8), Some(80));
        
        // Test configuration validation
        dynbitrate.set_property("encoder", &encoder_stub);
        dynbitrate.set_property("rist", &riststats_mock);
        dynbitrate.set_property("min-kbps", *min_kbps);
        dynbitrate.set_property("max-kbps", *max_kbps);
        dynbitrate.set_property("step-kbps", *step_kbps);
        dynbitrate.set_property("target-loss-pct", *target_loss);
        
        // Verify properties were set correctly
        let actual_min: u32 = dynbitrate.property("min-kbps");
        let actual_max: u32 = dynbitrate.property("max-kbps");
        let actual_step: u32 = dynbitrate.property("step-kbps");
        let actual_target: f64 = dynbitrate.property("target-loss-pct");
        
        assert_eq!(actual_min, *min_kbps, "min-kbps should match configured value");
        assert_eq!(actual_max, *max_kbps, "max-kbps should match configured value"); 
        assert_eq!(actual_step, *step_kbps, "step-kbps should match configured value");
        assert!((actual_target - target_loss).abs() < 0.001, "target-loss-pct should match configured value");
        
        let pipeline = gstreamer::Pipeline::new();
        pipeline.add_many([&dynbitrate, &encoder_stub, &riststats_mock]).unwrap();
        
        // Test element operation with this configuration
        pipeline.set_state(gstreamer::State::Playing).unwrap();
        std::thread::sleep(Duration::from_millis(100));
        pipeline.set_state(gstreamer::State::Null).unwrap();
        
        println!("  {} configuration validated", description);
    }
    
    println!("dynbitrate configuration validation test passed");
}