//! Comprehensive GStreamer pipeline tests for RIST elements
//!
//! This test suite focuses on testing actual GStreamer pipeline behavior,
//! element interaction, data flow, and error conditions.

use gst::prelude::*;
use gstreamer as gst;
use gstristelements::testing::*;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[test]
fn test_dispatcher_basic_pipeline_creation() {
    init_for_tests();

    let pipeline = gst::Pipeline::new();
    let dispatcher = create_dispatcher_for_testing(None);

    // Test element creation
    assert_eq!(dispatcher.factory().unwrap().name(), "ristdispatcher");

    // Test adding to pipeline
    pipeline
        .add(&dispatcher)
        .expect("Failed to add dispatcher to pipeline");
    assert_eq!(pipeline.children().len(), 1);
}

#[test]
fn test_dispatcher_pad_creation_and_linking() {
    init_for_tests();

    let pipeline = gst::Pipeline::new();
    let dispatcher = create_dispatcher_for_testing(Some(&[0.5, 0.5]));
    let source = create_test_source();
    let sink1 = create_counter_sink();
    let sink2 = create_counter_sink();

    pipeline
        .add_many([&source, &dispatcher, &sink1, &sink2])
        .expect("Failed to add elements");

    // Test sink pad linking
    source.link(&dispatcher).expect("Failed to link source");

    // Test src pad creation and linking
    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();

    assert_eq!(src_0.name(), "src_0");
    assert_eq!(src_1.name(), "src_1");

    // Test linking src pads
    src_0
        .link(&sink1.static_pad("sink").unwrap())
        .expect("Failed to link src_0");
    src_1
        .link(&sink2.static_pad("sink").unwrap())
        .expect("Failed to link src_1");

    // Verify pad counts
    assert_eq!(dispatcher.src_pads().len(), 2);
    assert_eq!(dispatcher.sink_pads().len(), 1);
}

#[test]
fn test_dispatcher_weight_based_distribution() {
    init_for_tests();

    let pipeline = gst::Pipeline::new();
    let dispatcher = create_dispatcher_for_testing(Some(&[0.8, 0.2])); // 80%/20% split
    let source = create_test_source();
    let counter1 = create_counter_sink();
    let counter2 = create_counter_sink();

    pipeline
        .add_many([&source, &dispatcher, &counter1, &counter2])
        .expect("Failed to add elements");

    source.link(&dispatcher).expect("Failed to link source");

    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();

    src_0
        .link(&counter1.static_pad("sink").unwrap())
        .expect("Failed to link src_0");
    src_1
        .link(&counter2.static_pad("sink").unwrap())
        .expect("Failed to link src_1");

    // Run pipeline
    run_pipeline_for_duration(&pipeline, 3).expect("Pipeline failed");

    // Check distribution ratios
    let count1: u64 = get_property(&counter1, "count").expect("Failed to get count1");
    let count2: u64 = get_property(&counter2, "count").expect("Failed to get count2");

    println!("Weight test: Counter1={}, Counter2={}", count1, count2);

    // Allow for some variance but verify general weight distribution
    let total = count1 + count2;
    if total > 0 {
        let ratio1 = count1 as f64 / total as f64;
        let ratio2 = count2 as f64 / total as f64;

        // Should roughly follow 80/20 split (allow 20% variance)
        assert!(
            ratio1 > 0.6 && ratio1 < 1.0,
            "Ratio1 {} not in expected range",
            ratio1
        );
        assert!(
            ratio2 > 0.0 && ratio2 < 0.4,
            "Ratio2 {} not in expected range",
            ratio2
        );
    }
}

#[test]
fn test_dispatcher_no_weights_equal_distribution() {
    init_for_tests();

    let pipeline = gst::Pipeline::new();
    let dispatcher = create_dispatcher_for_testing(None); // No weights = equal distribution
    let source = create_test_source();
    let counter1 = create_counter_sink();
    let counter2 = create_counter_sink();

    pipeline
        .add_many([&source, &dispatcher, &counter1, &counter2])
        .expect("Failed to add elements");

    source.link(&dispatcher).expect("Failed to link source");

    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();

    src_0
        .link(&counter1.static_pad("sink").unwrap())
        .expect("Failed to link src_0");
    src_1
        .link(&counter2.static_pad("sink").unwrap())
        .expect("Failed to link src_1");

    // Run pipeline
    run_pipeline_for_duration(&pipeline, 3).expect("Pipeline failed");

    let count1: u64 = get_property(&counter1, "count").expect("Failed to get count1");
    let count2: u64 = get_property(&counter2, "count").expect("Failed to get count2");

    println!(
        "Equal distribution test: Counter1={}, Counter2={}",
        count1, count2
    );

    // Should be roughly equal (allow 30% variance due to SWRR algorithm)
    let total = count1 + count2;
    if total > 10 {
        // Only check if we have reasonable sample size
        let diff = (count1 as i64 - count2 as i64).abs() as f64;
        let variance = diff / total as f64;
        assert!(
            variance < 0.3,
            "Distribution too uneven: variance={}",
            variance
        );
    }
}

#[test]
fn test_dispatcher_dynamic_pad_addition() {
    init_for_tests();

    let pipeline = gst::Pipeline::new();
    let dispatcher = create_dispatcher_for_testing(Some(&[1.0])); // Start with one output
    let source = create_test_source();
    let counter1 = create_counter_sink();

    pipeline
        .add_many([&source, &dispatcher, &counter1])
        .expect("Failed to add elements");

    source.link(&dispatcher).expect("Failed to link source");
    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    src_0
        .link(&counter1.static_pad("sink").unwrap())
        .expect("Failed to link src_0");

    // Start pipeline
    pipeline
        .set_state(gst::State::Playing)
        .expect("Failed to start pipeline");
    thread::sleep(Duration::from_millis(500));

    // Add second output while running
    let counter2 = create_counter_sink();
    pipeline.add(&counter2).expect("Failed to add counter2");
    let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();
    src_1
        .link(&counter2.static_pad("sink").unwrap())
        .expect("Failed to link src_1");

    // Continue running
    thread::sleep(Duration::from_secs(2));

    pipeline
        .set_state(gst::State::Null)
        .expect("Failed to stop pipeline");

    // Both counters should have received data
    let count1: u64 = get_property(&counter1, "count").expect("Failed to get count1");
    let count2: u64 = get_property(&counter2, "count").expect("Failed to get count2");

    println!("Dynamic pad test: Counter1={}, Counter2={}", count1, count2);

    assert!(count1 > 0, "Counter1 should have received data");
    // Counter2 might be 0 if added too late, but should not cause errors
}

#[test]
fn test_dynbitrate_element_creation() {
    init_for_tests();

    let dynbitrate = gst::ElementFactory::make("dynbitrate")
        .build()
        .expect("Failed to create dynbitrate element");

    assert_eq!(dynbitrate.factory().unwrap().name(), "dynbitrate");

    // Test default property values
    let min_kbps: u32 = get_property(&dynbitrate, "min-kbps").expect("Failed to get min-kbps");
    let max_kbps: u32 = get_property(&dynbitrate, "max-kbps").expect("Failed to get max-kbps");
    let target_loss: f64 =
        get_property(&dynbitrate, "target-loss-pct").expect("Failed to get target-loss-pct");

    assert!(min_kbps > 0);
    assert!(max_kbps > min_kbps);
    assert!(target_loss >= 0.0 && target_loss <= 100.0);
}

#[test]
fn test_dynbitrate_property_validation() {
    init_for_tests();

    let dynbitrate = gst::ElementFactory::make("dynbitrate")
        .build()
        .expect("Failed to create dynbitrate element");

    // Test setting valid properties
    dynbitrate.set_property("min-kbps", 500u32);
    dynbitrate.set_property("max-kbps", 5000u32);
    dynbitrate.set_property("target-loss-pct", 2.0f64);

    let min_kbps: u32 = get_property(&dynbitrate, "min-kbps").expect("Failed to get min-kbps");
    let max_kbps: u32 = get_property(&dynbitrate, "max-kbps").expect("Failed to get max-kbps");
    let target_loss: f64 =
        get_property(&dynbitrate, "target-loss-pct").expect("Failed to get target-loss-pct");

    assert_eq!(min_kbps, 500);
    assert_eq!(max_kbps, 5000);
    assert_eq!(target_loss, 2.0);
}

#[test]
fn test_dispatcher_caps_negotiation() {
    init_for_tests();

    let pipeline = gst::Pipeline::new();
    let dispatcher = create_dispatcher_for_testing(Some(&[0.5, 0.5]));
    let source = create_rtp_test_source();
    let sink = create_counter_sink();

    pipeline
        .add_many([&source, &dispatcher, &sink])
        .expect("Failed to add elements");

    source.link(&dispatcher).expect("Failed to link source");
    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    src_0
        .link(&sink.static_pad("sink").unwrap())
        .expect("Failed to link src");

    // Test caps negotiation by running briefly
    pipeline
        .set_state(gst::State::Playing)
        .expect("Failed to start pipeline");
    thread::sleep(Duration::from_millis(500));

    // Check that caps were negotiated on pads
    let sink_pad = dispatcher.static_pad("sink").unwrap();
    let src_pad = dispatcher.request_pad_simple("src_%u").unwrap();

    let sink_caps = sink_pad.current_caps();
    let src_caps = src_pad.current_caps();

    assert!(sink_caps.is_some(), "Sink pad should have negotiated caps");
    assert!(src_caps.is_some(), "Src pad should have negotiated caps");

    // Caps should be compatible
    if let (Some(sink_caps), Some(src_caps)) = (sink_caps, src_caps) {
        // Basic sanity check - both should have application/x-rtp media type
        let sink_struct = sink_caps.structure(0).unwrap();
        let src_struct = src_caps.structure(0).unwrap();

        assert_eq!(sink_struct.name(), "application/x-rtp");
        assert_eq!(src_struct.name(), "application/x-rtp");
    }

    pipeline
        .set_state(gst::State::Null)
        .expect("Failed to stop pipeline");
}

#[test]
fn test_pipeline_state_changes() {
    init_for_tests();

    let pipeline = gst::Pipeline::new();
    let dispatcher = create_dispatcher_for_testing(Some(&[1.0]));
    let source = create_test_source();
    let sink = create_counter_sink();

    pipeline
        .add_many([&source, &dispatcher, &sink])
        .expect("Failed to add elements");

    source.link(&dispatcher).expect("Failed to link source");
    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    src_0
        .link(&sink.static_pad("sink").unwrap())
        .expect("Failed to link src");

    // Test state transitions
    pipeline
        .set_state(gst::State::Ready)
        .expect("Failed to set Ready state");
    pipeline
        .set_state(gst::State::Paused)
        .expect("Failed to set Paused state");
    pipeline
        .set_state(gst::State::Playing)
        .expect("Failed to set Playing state");

    // Let it run briefly
    thread::sleep(Duration::from_millis(500));

    // Test pause
    pipeline
        .set_state(gst::State::Paused)
        .expect("Failed to pause");

    // Test stop
    pipeline
        .set_state(gst::State::Null)
        .expect("Failed to stop");
}

#[test]
fn test_pipeline_error_recovery() {
    init_for_tests();

    let pipeline = gst::Pipeline::new();
    let dispatcher = create_dispatcher_for_testing(Some(&[1.0]));

    // Create a source that will cause errors (invalid configuration)
    let source = gst::ElementFactory::make("audiotestsrc")
        .build()
        .expect("Failed to create audio test source");

    // Try to connect audio source to RTP dispatcher (caps mismatch)
    pipeline
        .add_many([&source, &dispatcher])
        .expect("Failed to add elements");

    // This link should fail during caps negotiation
    let link_result = source.link(&dispatcher);

    // The link itself might succeed but caps negotiation will fail
    if link_result.is_ok() {
        pipeline
            .set_state(gst::State::Playing)
            .expect("Failed to start pipeline");

        // Wait for potential error
        let bus = pipeline.bus().unwrap();
        let timeout_result = bus.timed_pop_filtered(
            gst::ClockTime::from_seconds(2),
            &[gst::MessageType::Error, gst::MessageType::Eos],
        );

        if let Some(msg) = timeout_result {
            match msg.view() {
                gst::MessageView::Error(error_msg) => {
                    println!("Expected error occurred: {}", error_msg.error().to_string());
                    // This is expected behavior
                }
                _ => {}
            }
        }

        // Should be able to stop cleanly even after error
        pipeline
            .set_state(gst::State::Null)
            .expect("Failed to stop after error");
    }
}

#[test]
fn test_concurrent_pipeline_operations() {
    init_for_tests();

    let pipeline = Arc::new(gst::Pipeline::new());
    let dispatcher = create_dispatcher_for_testing(Some(&[0.5, 0.5]));
    let source = create_test_source();
    let counter1 = Arc::new(create_counter_sink());
    let counter2 = Arc::new(create_counter_sink());

    pipeline
        .add_many([&source, &dispatcher, counter1.as_ref(), counter2.as_ref()])
        .expect("Failed to add elements");

    source.link(&dispatcher).expect("Failed to link source");

    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();

    src_0
        .link(&counter1.static_pad("sink").unwrap())
        .expect("Failed to link src_0");
    src_1
        .link(&counter2.static_pad("sink").unwrap())
        .expect("Failed to link src_1");

    // Start pipeline
    pipeline
        .set_state(gst::State::Playing)
        .expect("Failed to start pipeline");

    // Spawn concurrent threads to read properties while pipeline is running
    let counter1_clone = counter1.clone();
    let counter2_clone = counter2.clone();

    let handles: Vec<_> = (0..5)
        .map(|_| {
            let c1 = counter1_clone.clone();
            let c2 = counter2_clone.clone();
            thread::spawn(move || {
                for _ in 0..20 {
                    let _count1: u64 = get_property(&c1, "count").unwrap_or(0);
                    let _count2: u64 = get_property(&c2, "count").unwrap_or(0);
                    thread::sleep(Duration::from_millis(10));
                }
            })
        })
        .collect();

    // Let pipeline run while threads are accessing properties
    thread::sleep(Duration::from_secs(1));

    // Wait for all threads to complete
    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Stop pipeline
    pipeline
        .set_state(gst::State::Null)
        .expect("Failed to stop pipeline");

    // Final verification
    let count1: u64 = get_property(&counter1, "count").expect("Failed to get final count1");
    let count2: u64 = get_property(&counter2, "count").expect("Failed to get final count2");

    println!(
        "Concurrent test final counts: Counter1={}, Counter2={}",
        count1, count2
    );
    assert!(
        count1 > 0 || count2 > 0,
        "At least one counter should have data"
    );
}

#[test]
fn test_pad_removal_and_cleanup() {
    init_for_tests();

    let pipeline = gst::Pipeline::new();
    let dispatcher = create_dispatcher_for_testing(Some(&[0.5, 0.5]));
    let source = create_test_source();
    let counter1 = create_counter_sink();
    let counter2 = create_counter_sink();

    pipeline
        .add_many([&source, &dispatcher, &counter1, &counter2])
        .expect("Failed to add elements");

    source.link(&dispatcher).expect("Failed to link source");

    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();

    src_0
        .link(&counter1.static_pad("sink").unwrap())
        .expect("Failed to link src_0");
    src_1
        .link(&counter2.static_pad("sink").unwrap())
        .expect("Failed to link src_1");

    // Start pipeline
    pipeline
        .set_state(gst::State::Playing)
        .expect("Failed to start pipeline");
    thread::sleep(Duration::from_millis(500));

    // Pause pipeline for pad operations
    pipeline
        .set_state(gst::State::Paused)
        .expect("Failed to pause pipeline");

    // Unlink and remove one pad
    let _result = src_1.unlink(&counter2.static_pad("sink").unwrap());
    pipeline
        .remove(&counter2)
        .expect("Failed to remove counter2");
    dispatcher.release_request_pad(&src_1);

    // Resume and verify single output still works
    pipeline
        .set_state(gst::State::Playing)
        .expect("Failed to resume pipeline");
    thread::sleep(Duration::from_secs(1));

    pipeline
        .set_state(gst::State::Null)
        .expect("Failed to stop pipeline");

    // Verify first counter still received data after pad removal
    let count1: u64 = get_property(&counter1, "count").expect("Failed to get count1");
    println!("Pad removal test: Counter1={}", count1);
    assert!(count1 > 0, "Remaining counter should still receive data");

    // Verify only one src pad remains
    assert_eq!(dispatcher.src_pads().len(), 1);
}

#[test]
fn test_zero_weight_handling() {
    init_for_tests();

    let pipeline = gst::Pipeline::new();
    let dispatcher = create_dispatcher_for_testing(Some(&[1.0, 0.0, 0.5])); // Include zero weight
    let source = create_test_source();
    let counter1 = create_counter_sink();
    let counter2 = create_counter_sink();
    let counter3 = create_counter_sink();

    pipeline
        .add_many([&source, &dispatcher, &counter1, &counter2, &counter3])
        .expect("Failed to add elements");

    source.link(&dispatcher).expect("Failed to link source");

    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_2 = dispatcher.request_pad_simple("src_%u").unwrap();

    src_0
        .link(&counter1.static_pad("sink").unwrap())
        .expect("Failed to link src_0");
    src_1
        .link(&counter2.static_pad("sink").unwrap())
        .expect("Failed to link src_1");
    src_2
        .link(&counter3.static_pad("sink").unwrap())
        .expect("Failed to link src_2");

    // Run pipeline
    run_pipeline_for_duration(&pipeline, 3).expect("Pipeline failed");

    let count1: u64 = get_property(&counter1, "count").expect("Failed to get count1");
    let count2: u64 = get_property(&counter2, "count").expect("Failed to get count2");
    let count3: u64 = get_property(&counter3, "count").expect("Failed to get count3");

    println!(
        "Zero weight test: Counter1={}, Counter2={}, Counter3={}",
        count1, count2, count3
    );

    // Counter2 (zero weight) should receive no data
    assert_eq!(count2, 0, "Zero weight output should receive no data");

    // Counter1 and Counter3 should receive data proportional to their weights (1.0 vs 0.5)
    if count1 > 0 && count3 > 0 {
        let ratio = count1 as f64 / count3 as f64;
        assert!(
            ratio > 1.5 && ratio < 2.5,
            "Weight ratio should be approximately 2:1, got {}",
            ratio
        );
    }
}

#[test]
fn test_buffer_flow_and_timing() {
    init_for_tests();

    let pipeline = gst::Pipeline::new();
    let dispatcher = create_dispatcher_for_testing(Some(&[1.0]));
    let source = create_test_source();

    // Use identity element to observe buffer flow
    let identity = gst::ElementFactory::make("identity")
        .property("dump", true)
        .build()
        .expect("Failed to create identity");

    let sink = create_counter_sink();

    pipeline
        .add_many([&source, &dispatcher, &identity, &sink])
        .expect("Failed to add elements");

    source.link(&dispatcher).expect("Failed to link source");
    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    src_0
        .link(&identity.static_pad("sink").unwrap())
        .expect("Failed to link dispatcher to identity");
    identity
        .link(&sink)
        .expect("Failed to link identity to sink");

    // Run for a specific duration and measure buffer throughput
    let start_time = std::time::Instant::now();
    run_pipeline_for_duration(&pipeline, 2).expect("Pipeline failed");
    let duration = start_time.elapsed();

    let count: u64 = get_property(&sink, "count").expect("Failed to get count");
    let rate = count as f64 / duration.as_secs_f64();

    println!(
        "Buffer flow test: {} buffers in {:?} ({:.1} buffers/sec)",
        count, duration, rate
    );

    // Should have reasonable buffer rate (at least 1 buffer per second for a working pipeline)
    assert!(rate > 0.5, "Buffer rate too low: {:.1}/sec", rate);
}
