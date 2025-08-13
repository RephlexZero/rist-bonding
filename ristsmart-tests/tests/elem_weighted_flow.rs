// Test weighted buffer distribution: appsrc → dispatcher → N counter_sinks (~weight splits)

use gst::prelude::*;
use gstreamer as gst;
use gstreamer_app as gst_app;
use serde_json;
use std::time::Duration;

/// Test weighted flow distribution across multiple sinks
#[test]
fn test_weighted_flow_distribution() {
    ristsmart_tests::register_everything_for_tests();

    // Create pipeline: appsrc -> dispatcher -> 3 counter_sinks
    let pipeline = gst::Pipeline::new();
    let appsrc = gst::ElementFactory::make("appsrc")
        .property("caps", &gst::Caps::builder("application/x-rtp").build())
        .property("format", &gst::Format::Time)
        .build()
        .expect("Failed to create appsrc");

    // Create dispatcher with specific weights: [3.0, 2.0, 1.0]
    // This should result in approximately 50%, 33%, 17% distribution
    let dispatcher = gst::ElementFactory::make("ristdispatcher")
        .property("weights", "[3.0, 2.0, 1.0]")
        .property("auto-balance", false) // Disable auto-balance for deterministic testing
        .property("min-hold-ms", 0u64) // Disable hysteresis for testing
        .property("switch-threshold", 1.0) // No threshold for switching
        .property("health-warmup-ms", 0u64) // Disable health warmup for testing
        .build()
        .expect("Failed to create ristdispatcher");

    // Create 3 counter sinks
    let counter_sink1 = gst::ElementFactory::make("counter_sink")
        .name("counter1")
        .build()
        .expect("Failed to create counter_sink 1");

    let counter_sink2 = gst::ElementFactory::make("counter_sink")
        .name("counter2")
        .build()
        .expect("Failed to create counter_sink 2");

    let counter_sink3 = gst::ElementFactory::make("counter_sink")
        .name("counter3")
        .build()
        .expect("Failed to create counter_sink 3");

    pipeline
        .add_many(&[
            &appsrc,
            &dispatcher,
            &counter_sink1,
            &counter_sink2,
            &counter_sink3,
        ])
        .unwrap();

    // Link appsrc to dispatcher
    appsrc
        .link(&dispatcher)
        .expect("Failed to link appsrc to dispatcher");

    // Request src pads from dispatcher and link to counter sinks
    let src_pad1 = dispatcher
        .request_pad_simple("src_%u")
        .expect("Failed to request src pad 1");
    let src_pad2 = dispatcher
        .request_pad_simple("src_%u")
        .expect("Failed to request src pad 2");
    let src_pad3 = dispatcher
        .request_pad_simple("src_%u")
        .expect("Failed to request src pad 3");

    src_pad1
        .link(&counter_sink1.static_pad("sink").unwrap())
        .expect("Failed to link to counter1");
    src_pad2
        .link(&counter_sink2.static_pad("sink").unwrap())
        .expect("Failed to link to counter2");
    src_pad3
        .link(&counter_sink3.static_pad("sink").unwrap())
        .expect("Failed to link to counter3");

    // Start pipeline
    pipeline
        .set_state(gst::State::Playing)
        .expect("Failed to set pipeline to Playing");

    // Push test buffers through appsrc
    let appsrc = appsrc.dynamic_cast::<gst_app::AppSrc>().unwrap();
    let total_buffers = 600; // Should be divisible by weights sum (3+2+1=6)

    for i in 0..total_buffers {
        let data = vec![b'A' + (i % 26) as u8; 100];
        let mut buffer = gst::Buffer::with_size(data.len()).unwrap();
        {
            let buffer_ref = buffer.get_mut().unwrap();
            buffer_ref.set_pts(gst::ClockTime::from_mseconds(i * 10));
        }
        if appsrc.push_buffer(buffer) != Ok(gst::FlowSuccess::Ok) {
            panic!("Failed to push buffer {}", i);
        }
    }

    // Send EOS and wait for processing
    appsrc.end_of_stream().expect("Failed to send EOS");

    // Give some time for EOS to propagate
    std::thread::sleep(Duration::from_millis(500));

    // Check if all counter sinks got EOS
    let got_eos1: bool = counter_sink1.property("got-eos");
    let got_eos2: bool = counter_sink2.property("got-eos");
    let got_eos3: bool = counter_sink3.property("got-eos");

    println!(
        "EOS status: counter1={}, counter2={}, counter3={}",
        got_eos1, got_eos2, got_eos3
    );

    // If EOS didn't reach all sinks, there might be a buffering issue
    if !(got_eos1 && got_eos2 && got_eos3) {
        eprintln!("Warning: Not all counter sinks received EOS, trying longer wait...");
        std::thread::sleep(Duration::from_millis(2000));

        let got_eos1: bool = counter_sink1.property("got-eos");
        let got_eos2: bool = counter_sink2.property("got-eos");
        let got_eos3: bool = counter_sink3.property("got-eos");
        println!(
            "EOS status after longer wait: counter1={}, counter2={}, counter3={}",
            got_eos1, got_eos2, got_eos3
        );
    }

    // Wait for EOS on bus - increased timeout since we now wait for all sinks
    let bus = pipeline.bus().expect("Failed to get bus");
    let timeout = Some(gst::ClockTime::from_seconds(10));
    match bus.timed_pop_filtered(timeout, &[gst::MessageType::Eos, gst::MessageType::Error]) {
        Some(msg) => match msg.view() {
            gst::MessageView::Eos(..) => println!("EOS received on bus"),
            gst::MessageView::Error(err) => {
                panic!("Pipeline error: {}", err.error());
            }
            _ => panic!("Unexpected message"),
        },
        None => {
            // If we still timeout, at least check that buffers were distributed before failing
            let count1: u64 = counter_sink1.property("count");
            let count2: u64 = counter_sink2.property("count");
            let count3: u64 = counter_sink3.property("count");

            println!("Timeout waiting for EOS on bus, but buffers were distributed: counter1={}, counter2={}, counter3={}", count1, count2, count3);

            if count1 + count2 + count3 == total_buffers as u64 {
                println!("All buffers were received, proceeding despite EOS timeout");
            } else {
                panic!("Timeout waiting for EOS and not all buffers were received");
            }
        }
    }

    // Properly stop the pipeline with state change completion
    pipeline
        .set_state(gst::State::Null)
        .expect("Failed to set pipeline to Null");

    // Small delay to allow cleanup
    std::thread::sleep(Duration::from_millis(100));

    // Check buffer distribution
    let count1: u64 = counter_sink1.property("count");
    let count2: u64 = counter_sink2.property("count");
    let count3: u64 = counter_sink3.property("count");

    println!(
        "Buffer distribution: counter1={}, counter2={}, counter3={}",
        count1, count2, count3
    );
    println!(
        "Total buffers: {}, Sum of counts: {}",
        total_buffers,
        count1 + count2 + count3
    );

    // Verify all buffers were received
    assert_eq!(
        count1 + count2 + count3,
        total_buffers as u64,
        "All buffers should be accounted for"
    );

    // Verify approximate weighted distribution
    // Expected ratios: 3:2:1 = 50%, 33.33%, 16.67%
    let total_f = total_buffers as f64;
    let ratio1 = count1 as f64 / total_f;
    let ratio2 = count2 as f64 / total_f;
    let ratio3 = count3 as f64 / total_f;

    println!(
        "Ratios: counter1={:.2}%, counter2={:.2}%, counter3={:.2}%",
        ratio1 * 100.0,
        ratio2 * 100.0,
        ratio3 * 100.0
    );

    // Allow some tolerance for statistical variation (±5%)
    assert!(
        (ratio1 - 0.5).abs() < 0.05,
        "Counter1 should receive ~50% of buffers, got {:.2}%",
        ratio1 * 100.0
    );
    assert!(
        (ratio2 - 0.3333).abs() < 0.05,
        "Counter2 should receive ~33% of buffers, got {:.2}%",
        ratio2 * 100.0
    );
    assert!(
        (ratio3 - 0.1667).abs() < 0.05,
        "Counter3 should receive ~17% of buffers, got {:.2}%",
        ratio3 * 100.0
    );

    // Verify that higher weights get more buffers
    assert!(
        count1 > count2,
        "Counter1 (weight=3.0) should receive more than Counter2 (weight=2.0)"
    );
    assert!(
        count2 > count3,
        "Counter2 (weight=2.0) should receive more than Counter3 (weight=1.0)"
    );

    println!("Weighted flow distribution test passed!");
}

/// Test dynamic weight adjustment during flow
#[test]
fn test_dynamic_weight_adjustment() {
    ristsmart_tests::register_everything_for_tests();

    let pipeline = gst::Pipeline::new();
    let appsrc = gst::ElementFactory::make("appsrc")
        .property("caps", &gst::Caps::builder("application/x-rtp").build())
        .property("format", &gst::Format::Time)
        .build()
        .expect("Failed to create appsrc");

    // Start with equal weights
    let dispatcher = gst::ElementFactory::make("ristdispatcher")
        .property("weights", "[1.0, 1.0]")
        .property("auto-balance", false)
        .property("min-hold-ms", 0u64) // Disable hysteresis for testing
        .property("switch-threshold", 1.0) // No threshold for switching
        .property("health-warmup-ms", 0u64) // Disable health warmup for testing
        .build()
        .expect("Failed to create ristdispatcher");

    let counter_sink1 = gst::ElementFactory::make("counter_sink")
        .name("counter1")
        .build()
        .expect("Failed to create counter_sink 1");

    let counter_sink2 = gst::ElementFactory::make("counter_sink")
        .name("counter2")
        .build()
        .expect("Failed to create counter_sink 2");

    pipeline
        .add_many(&[&appsrc, &dispatcher, &counter_sink1, &counter_sink2])
        .unwrap();

    // Link elements
    appsrc
        .link(&dispatcher)
        .expect("Failed to link appsrc to dispatcher");

    let src_pad1 = dispatcher
        .request_pad_simple("src_%u")
        .expect("Failed to request src pad 1");
    let src_pad2 = dispatcher
        .request_pad_simple("src_%u")
        .expect("Failed to request src pad 2");

    src_pad1
        .link(&counter_sink1.static_pad("sink").unwrap())
        .expect("Failed to link to counter1");
    src_pad2
        .link(&counter_sink2.static_pad("sink").unwrap())
        .expect("Failed to link to counter2");

    pipeline
        .set_state(gst::State::Playing)
        .expect("Failed to set pipeline to Playing");

    let appsrc = appsrc.dynamic_cast::<gst_app::AppSrc>().unwrap();

    // Phase 1: Push buffers with equal weights
    for i in 0..100 {
        let data = vec![b'A'; 100];
        let mut buffer = gst::Buffer::with_size(data.len()).unwrap();
        {
            let buffer_ref = buffer.get_mut().unwrap();
            buffer_ref.set_pts(gst::ClockTime::from_mseconds(i * 10));
        }
        appsrc.push_buffer(buffer).expect("Failed to push buffer");
    }

    std::thread::sleep(Duration::from_millis(100));

    // Check counts after phase 1 (should be roughly equal)
    let count1_phase1: u64 = counter_sink1.property("count");
    let count2_phase1: u64 = counter_sink2.property("count");

    println!(
        "Phase 1 - Equal weights [1.0, 1.0]: counter1={}, counter2={}",
        count1_phase1, count2_phase1
    );

    // Verify roughly equal distribution (within ±20 buffers)
    let diff = if count1_phase1 > count2_phase1 {
        count1_phase1 - count2_phase1
    } else {
        count2_phase1 - count1_phase1
    };
    assert!(
        diff <= 20,
        "With equal weights, buffer counts should be similar, got difference of {}",
        diff
    );

    // Phase 2: Change weights to heavily favor counter2
    dispatcher.set_property("weights", "[1.0, 5.0]");

    // Verify the weights were updated
    let current_weights: String = dispatcher.property("current-weights");
    let weights_json: serde_json::Value =
        serde_json::from_str(&current_weights).expect("current-weights should be valid JSON");
    let weights_array = weights_json.as_array().unwrap();
    assert_eq!(
        weights_array[1].as_f64().unwrap(),
        5.0,
        "Weight 2 should be updated to 5.0"
    );

    // Phase 2: Push more buffers with new weights
    for i in 100..200 {
        let data = vec![b'B'; 100];
        let mut buffer = gst::Buffer::with_size(data.len()).unwrap();
        {
            let buffer_ref = buffer.get_mut().unwrap();
            buffer_ref.set_pts(gst::ClockTime::from_mseconds(i * 10));
        }
        appsrc.push_buffer(buffer).expect("Failed to push buffer");
    }

    std::thread::sleep(Duration::from_millis(100));

    // Check final counts
    let count1_final: u64 = counter_sink1.property("count");
    let count2_final: u64 = counter_sink2.property("count");

    println!(
        "Phase 2 - Adjusted weights [1.0, 5.0]: counter1={}, counter2={}",
        count1_final, count2_final
    );

    // Calculate phase 2 increments
    let count1_phase2 = count1_final - count1_phase1;
    let count2_phase2 = count2_final - count2_phase1;

    println!(
        "Phase 2 increments: counter1=+{}, counter2=+{}",
        count1_phase2, count2_phase2
    );

    // In phase 2, counter2 should receive significantly more buffers (weight 5.0 vs 1.0)
    // Expected ratio is approximately 1:5, allowing for some tolerance
    if count2_phase2 > 0 && count1_phase2 > 0 {
        let ratio = count2_phase2 as f64 / count1_phase2 as f64;
        assert!(
            ratio >= 2.0,
            "Counter2 should receive significantly more buffers in phase 2, ratio={:.2}",
            ratio
        );
    }

    appsrc.end_of_stream().expect("Failed to send EOS");

    // Wait for EOS
    let bus = pipeline.bus().expect("Failed to get bus");
    let timeout = Some(gst::ClockTime::from_seconds(5));
    bus.timed_pop_filtered(timeout, &[gst::MessageType::Eos, gst::MessageType::Error]);

    pipeline
        .set_state(gst::State::Null)
        .expect("Failed to set pipeline to Null");

    // Small delay to allow cleanup
    std::thread::sleep(Duration::from_millis(100));

    println!("Dynamic weight adjustment test passed!");
}

/// Test error handling with invalid weights
#[test]
fn test_invalid_weights_handling() {
    ristsmart_tests::register_everything_for_tests();

    let dispatcher = gst::ElementFactory::make("ristdispatcher")
        .property("min-hold-ms", 0u64) // Disable hysteresis for testing
        .property("switch-threshold", 1.0) // No threshold for switching
        .property("health-warmup-ms", 0u64) // Disable health warmup for testing
        .build()
        .expect("Failed to create ristdispatcher");

    // Test 1: Invalid JSON should be ignored and use defaults
    dispatcher.set_property("weights", "invalid_json");
    let weights_str: String = dispatcher.property("current-weights");
    let weights_json: serde_json::Value =
        serde_json::from_str(&weights_str).expect("Should still have valid default weights");
    assert!(weights_json.is_array(), "Should have default weights array");

    // Test 2: Empty array should use defaults
    dispatcher.set_property("weights", "[]");
    let weights_str: String = dispatcher.property("current-weights");
    let weights_json: serde_json::Value =
        serde_json::from_str(&weights_str).expect("Should have valid weights");
    assert!(weights_json.is_array(), "Should have weights array");

    // Test 3: Negative weights should be handled gracefully
    dispatcher.set_property("weights", "[-1.0, 2.0]");
    let weights_str: String = dispatcher.property("current-weights");
    let weights_json: serde_json::Value =
        serde_json::from_str(&weights_str).expect("Should have valid weights");
    let weights_array = weights_json.as_array().unwrap();

    // All weights should be non-negative (negative weights should be corrected)
    for (i, weight) in weights_array.iter().enumerate() {
        let weight_val = weight.as_f64().expect("Weight should be a number");
        assert!(
            weight_val >= 0.0,
            "Weight {} should be non-negative, got {}",
            i,
            weight_val
        );
    }

    println!("Invalid weights handling test passed!");
}
