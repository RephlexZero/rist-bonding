//! Weighted flow distribution tests
//!
//! These tests verify that the dispatcher distributes buffers
//! according to specified weights, ensuring proper load balancing
//! across multiple RIST outputs.

use gst::prelude::*;
use gstreamer as gst;
use gstristelements::testing::*;

#[test]
fn test_weighted_distribution_basic() {
    init_for_tests();

    println!("=== Basic Weighted Distribution Test ===");

    let source = create_test_source();
    let dispatcher = create_dispatcher_for_testing(Some(&[0.8, 0.2])); // 80% vs 20%
    let counter1 = create_counter_sink();
    let counter2 = create_counter_sink();

    let pipeline = gst::Pipeline::new();
    pipeline
        .add_many([&source, &dispatcher, &counter1, &counter2])
        .expect("Failed to add elements to pipeline");

    // Link elements
    source.link(&dispatcher).unwrap();

    let src_pad1 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_pad2 = dispatcher.request_pad_simple("src_%u").unwrap();

    src_pad1
        .link(&counter1.static_pad("sink").unwrap())
        .unwrap();
    src_pad2
        .link(&counter2.static_pad("sink").unwrap())
        .unwrap();

    run_pipeline_for_duration(&pipeline, 3).expect("Weighted flow pipeline failed");

    let count1: u64 = get_property(&counter1, "count").unwrap();
    let count2: u64 = get_property(&counter2, "count").unwrap();

    println!("Counter 1 (0.8 weight): {} buffers", count1);
    println!("Counter 2 (0.2 weight): {} buffers", count2);

    // Verify weighted distribution (allowing for some variance)
    let total = count1 + count2;
    if total > 0 {
        let ratio1 = count1 as f64 / total as f64;
        let ratio2 = count2 as f64 / total as f64;

        println!("Actual ratios: {:.2} vs {:.2}", ratio1, ratio2);

        // Allow 10% variance from expected ratios
        assert!(
            (ratio1 - 0.8).abs() < 0.1,
            "First output should get ~80% of traffic"
        );
        assert!(
            (ratio2 - 0.2).abs() < 0.1,
            "Second output should get ~20% of traffic"
        );
    }

    println!("✅ Weighted distribution test completed");
}

#[test]
fn test_equal_weights() {
    init_for_tests();

    println!("=== Equal Weights Test ===");

    let source = create_test_source();
    let dispatcher = create_dispatcher_for_testing(Some(&[0.5, 0.5])); // Equal weights
    let counter1 = create_counter_sink();
    let counter2 = create_counter_sink();

    let pipeline = gst::Pipeline::new();
    pipeline
        .add_many([&source, &dispatcher, &counter1, &counter2])
        .expect("Failed to add elements to pipeline");

    // Link elements
    source.link(&dispatcher).unwrap();

    let src_pad1 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_pad2 = dispatcher.request_pad_simple("src_%u").unwrap();

    src_pad1
        .link(&counter1.static_pad("sink").unwrap())
        .unwrap();
    src_pad2
        .link(&counter2.static_pad("sink").unwrap())
        .unwrap();

    run_pipeline_for_duration(&pipeline, 3).expect("Equal weight pipeline failed");

    let count1: u64 = get_property(&counter1, "count").unwrap();
    let count2: u64 = get_property(&counter2, "count").unwrap();

    println!("Counter 1: {} buffers", count1);
    println!("Counter 2: {} buffers", count2);

    // For equal weights, expect roughly equal distribution
    let total = count1 + count2;
    if total > 0 {
        let ratio1 = count1 as f64 / total as f64;
        assert!(
            (ratio1 - 0.5).abs() < 0.15,
            "Equal weights should distribute roughly equally"
        );
    }

    println!("✅ Equal weights test completed");
}
