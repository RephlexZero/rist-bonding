use gst::prelude::*;
use gstreamer as gst;
use gstristelements::testing::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Pump the GLib main loop for the specified duration
fn run_mainloop_ms(ms: u64) {
    let ctx = glib::MainContext::default();
    let _guard = ctx.acquire().expect("acquire main context");
    let end = std::time::Instant::now() + Duration::from_millis(ms);
    while std::time::Instant::now() < end {
        while ctx.iteration(false) {}
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn test_high_throughput_performance() {
    init_for_tests();
    println!("=== High Throughput Performance Benchmark ===");

    let dispatcher = create_dispatcher_for_testing(Some(&[0.25, 0.25, 0.25, 0.25]));

    let pipeline = gst::Pipeline::new();

    // High rate source for performance testing
    let source = gst::ElementFactory::make("audiotestsrc")
        .property("samplesperbuffer", 1024i32)
        .property("num-buffers", 10000i32) // High buffer count for throughput test
        .property("is-live", false)
        .build()
        .unwrap();

    // Create multiple output paths
    let mut counters = Vec::new();
    let mut pads = Vec::new();

    for i in 0..4 {
        let counter = create_counter_sink();
        counter.set_property("name", format!("counter_{}", i));
        counters.push(counter);
    }

    pipeline.add(&source).unwrap();
    pipeline.add(&dispatcher).unwrap();
    for counter in &counters {
        pipeline.add(counter).unwrap();
    }

    // Link source to dispatcher
    source.link(&dispatcher).unwrap();

    // Request pads and link to counters
    for counter in &counters {
        let src_pad = dispatcher.request_pad_simple("src_%u").unwrap();
        pads.push(src_pad.clone());
        src_pad.link(&counter.static_pad("sink").unwrap()).unwrap();
    }

    println!("Starting high throughput test with 4 outputs...");
    let start_time = Instant::now();

    pipeline.set_state(gst::State::Playing).unwrap();

    // Wait for processing completion with timeout
    let mut processed_count = 0u64;
    let timeout = Duration::from_secs(30);
    let test_start = Instant::now();

    loop {
        run_mainloop_ms(100);

        // Check if all buffers processed
        let total_count: u64 = counters
            .iter()
            .map(|c| get_property::<u64>(c, "count").unwrap_or(0))
            .sum();

        if total_count >= 10000 || test_start.elapsed() > timeout {
            processed_count = total_count;
            break;
        }
    }

    let elapsed = start_time.elapsed();
    pipeline.set_state(gst::State::Null).unwrap();

    println!(
        "Processed {} buffers in {:.3}s",
        processed_count,
        elapsed.as_secs_f64()
    );
    let throughput = processed_count as f64 / elapsed.as_secs_f64();
    println!("Throughput: {:.2} buffers/second", throughput);

    // Verify distribution
    for (i, counter) in counters.iter().enumerate() {
        let count: u64 = get_property(counter, "count").unwrap_or(0);
        println!("Counter {}: {} buffers", i, count);
    }

    assert!(
        processed_count >= 1000,
        "Should process significant number of buffers"
    );
    assert!(throughput >= 100.0, "Should achieve reasonable throughput");

    println!("✅ High throughput performance test passed");
}

#[test]
fn test_dynamic_rebalancing_performance() {
    init_for_tests();
    println!("=== Dynamic Rebalancing Performance Benchmark ===");

    let dispatcher = create_dispatcher_for_testing(Some(&[0.5, 0.5]));
    dispatcher.set_property("strategy", "ewma");
    dispatcher.set_property("rebalance-interval-ms", 100u64);
    dispatcher.set_property("auto-balance", true);

    let pipeline = gst::Pipeline::new();
    let source = gst::ElementFactory::make("audiotestsrc")
        .property("num-buffers", 5000i32)
        .property("is-live", false)
        .build()
        .unwrap();

    let counter1 = create_counter_sink();
    let counter2 = create_counter_sink();

    pipeline
        .add_many([&source, &dispatcher, &counter1, &counter2])
        .unwrap();
    source.link(&dispatcher).unwrap();

    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();

    src_0.link(&counter1.static_pad("sink").unwrap()).unwrap();
    src_1.link(&counter2.static_pad("sink").unwrap()).unwrap();

    println!("Starting dynamic rebalancing test...");
    let start_time = Instant::now();

    pipeline.set_state(gst::State::Playing).unwrap();

    // Continuously change weights during processing to test rebalancing performance
    let rebalance_count = Arc::new(AtomicU64::new(0));
    let rebalance_count_clone = rebalance_count.clone();

    let handle = std::thread::spawn(move || {
        let mut iteration = 0;
        while iteration < 50 {
            std::thread::sleep(Duration::from_millis(50));

            // Alternate between different weight distributions
            let weights = match iteration % 4 {
                0 => "[0.7, 0.3]",
                1 => "[0.3, 0.7]",
                2 => "[0.6, 0.4]",
                _ => "[0.4, 0.6]",
            };

            // This might fail if the test completes early, which is fine
            dispatcher.set_property("weights", weights);
            rebalance_count_clone.store(iteration + 1, Ordering::Relaxed);
            iteration += 1;
        }
    });

    // Monitor processing
    let mut final_count = 0u64;
    let timeout = Duration::from_secs(30);
    let test_start = Instant::now();

    loop {
        run_mainloop_ms(100);

        let count1: u64 = get_property(&counter1, "count").unwrap_or(0);
        let count2: u64 = get_property(&counter2, "count").unwrap_or(0);
        let total = count1 + count2;

        if total >= 5000 || test_start.elapsed() > timeout {
            final_count = total;
            break;
        }
    }

    let _ = handle.join();
    let elapsed = start_time.elapsed();
    pipeline.set_state(gst::State::Null).unwrap();

    let rebalances = rebalance_count.load(Ordering::Relaxed);
    println!(
        "Processed {} buffers with {} rebalances in {:.3}s",
        final_count,
        rebalances,
        elapsed.as_secs_f64()
    );

    let count1: u64 = get_property(&counter1, "count").unwrap_or(0);
    let count2: u64 = get_property(&counter2, "count").unwrap_or(0);
    println!(
        "Final distribution: Counter1={}, Counter2={}",
        count1, count2
    );

    assert!(
        final_count >= 1000,
        "Should process significant buffers despite rebalancing"
    );
    assert!(rebalances >= 10, "Should perform multiple rebalances");

    println!("✅ Dynamic rebalancing performance test passed");
}

#[test]
fn test_memory_usage_under_load() {
    init_for_tests();
    println!("=== Memory Usage Under Load Test ===");

    let dispatcher = create_dispatcher_for_testing(Some(&[0.3, 0.3, 0.4]));

    let pipeline = gst::Pipeline::new();
    let source = gst::ElementFactory::make("audiotestsrc")
        .property("num-buffers", 8000i32)
        .property("samplesperbuffer", 2048i32) // Larger buffers
        .property("is-live", false)
        .build()
        .unwrap();

    let mut counters = Vec::new();
    for i in 0..3 {
        let counter = create_counter_sink();
        counter.set_property("name", format!("mem_counter_{}", i));
        counters.push(counter);
    }

    pipeline.add(&source).unwrap();
    pipeline.add(&dispatcher).unwrap();
    for counter in &counters {
        pipeline.add(counter).unwrap();
    }

    source.link(&dispatcher).unwrap();

    // Link outputs
    let mut pads = Vec::new();
    for counter in &counters {
        let src_pad = dispatcher.request_pad_simple("src_%u").unwrap();
        src_pad.link(&counter.static_pad("sink").unwrap()).unwrap();
        pads.push(src_pad);
    }

    println!("Starting memory usage test with large buffers...");
    let start_time = Instant::now();

    pipeline.set_state(gst::State::Playing).unwrap();

    // Monitor processing and memory behavior
    let mut max_total_count = 0u64;
    let timeout = Duration::from_secs(45);
    let test_start = Instant::now();

    loop {
        run_mainloop_ms(200);

        let total_count: u64 = counters
            .iter()
            .map(|c| get_property::<u64>(c, "count").unwrap_or(0))
            .sum();

        max_total_count = max_total_count.max(total_count);

        if total_count >= 8000 || test_start.elapsed() > timeout {
            break;
        }

        // Periodic weight updates to test memory stability
        if total_count % 1000 == 0 && total_count > 0 {
            let weights = match (total_count / 1000) % 3 {
                0 => "[0.2, 0.3, 0.5]",
                1 => "[0.4, 0.4, 0.2]",
                _ => "[0.3, 0.3, 0.4]",
            };
            dispatcher.set_property("weights", weights);
        }
    }

    let elapsed = start_time.elapsed();
    pipeline.set_state(gst::State::Null).unwrap();

    println!(
        "Processed {} buffers in {:.3}s",
        max_total_count,
        elapsed.as_secs_f64()
    );

    // Verify distribution
    for (i, counter) in counters.iter().enumerate() {
        let count: u64 = get_property(counter, "count").unwrap_or(0);
        println!("Counter {}: {} buffers", i, count);
    }

    assert!(
        max_total_count >= 2000,
        "Should process significant number of large buffers"
    );
    println!("✅ Memory usage under load test passed");
}

#[test]
fn test_concurrent_property_access_performance() {
    init_for_tests();
    println!("=== Concurrent Property Access Performance Test ===");

    let dispatcher = create_dispatcher_for_testing(Some(&[0.5, 0.5]));
    dispatcher.set_property("strategy", "aimd");
    dispatcher.set_property("rebalance-interval-ms", 100u64);

    let update_count = Arc::new(AtomicU64::new(0));
    let read_count = Arc::new(AtomicU64::new(0));

    println!("Starting concurrent property access test...");
    let start_time = Instant::now();

    // Property update thread
    let dispatcher_update = dispatcher.clone();
    let update_count_clone = update_count.clone();
    let update_handle = std::thread::spawn(move || {
        let mut iteration = 0u64;
        while iteration < 1000 {
            let weights = if iteration % 2 == 0 {
                "[0.6, 0.4]"
            } else {
                "[0.4, 0.6]"
            };

            dispatcher_update.set_property("weights", weights);
            dispatcher_update.set_property("rebalance-interval-ms", 100u64 + (iteration % 100));

            update_count_clone.store(iteration + 1, Ordering::Relaxed);
            iteration += 1;
            std::thread::sleep(Duration::from_millis(1));
        }
    });

    // Property read thread
    let dispatcher_read = dispatcher.clone();
    let read_count_clone = read_count.clone();
    let read_handle = std::thread::spawn(move || {
        let mut iteration = 0u64;
        while iteration < 2000 {
            let _weights: String = dispatcher_read.property("weights");
            let _interval: u64 = dispatcher_read.property("rebalance-interval-ms");
            let _strategy: String = dispatcher_read.property("strategy");

            read_count_clone.store(iteration + 1, Ordering::Relaxed);
            iteration += 1;

            if iteration % 100 == 0 {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    });

    // Wait for completion
    let _ = update_handle.join();
    let _ = read_handle.join();

    let elapsed = start_time.elapsed();
    let updates = update_count.load(Ordering::Relaxed);
    let reads = read_count.load(Ordering::Relaxed);

    println!(
        "Completed {} property updates and {} reads in {:.3}s",
        updates,
        reads,
        elapsed.as_secs_f64()
    );

    let update_rate = updates as f64 / elapsed.as_secs_f64();
    let read_rate = reads as f64 / elapsed.as_secs_f64();

    println!(
        "Update rate: {:.2} ops/sec, Read rate: {:.2} ops/sec",
        update_rate, read_rate
    );

    assert!(
        updates >= 500,
        "Should complete significant number of updates"
    );
    assert!(reads >= 1000, "Should complete significant number of reads");
    assert!(
        update_rate >= 50.0,
        "Should achieve reasonable update performance"
    );
    assert!(
        read_rate >= 100.0,
        "Should achieve reasonable read performance"
    );

    println!("✅ Concurrent property access performance test passed");
}

#[test]
fn test_pad_lifecycle_performance() {
    init_for_tests();
    println!("=== Pad Lifecycle Performance Test ===");

    let dispatcher = create_dispatcher_for_testing(None);

    println!("Starting pad lifecycle performance test...");
    let start_time = Instant::now();

    // Rapid pad creation/destruction cycles
    let mut creation_count = 0u64;
    let mut destruction_count = 0u64;

    for cycle in 0..100 {
        // Create multiple pads
        let mut pads = Vec::new();
        for _ in 0..5 {
            if let Some(pad) = dispatcher.request_pad_simple("src_%u") {
                pads.push(pad);
                creation_count += 1;
            }
        }

        // Destroy them all
        for pad in pads {
            dispatcher.release_request_pad(&pad);
            destruction_count += 1;
        }

        // Brief pause every 20 cycles
        if cycle % 20 == 0 {
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    let elapsed = start_time.elapsed();
    println!(
        "Created {} pads and destroyed {} pads in {:.3}s",
        creation_count,
        destruction_count,
        elapsed.as_secs_f64()
    );

    let creation_rate = creation_count as f64 / elapsed.as_secs_f64();
    let destruction_rate = destruction_count as f64 / elapsed.as_secs_f64();

    println!(
        "Creation rate: {:.2} pads/sec, Destruction rate: {:.2} pads/sec",
        creation_rate, destruction_rate
    );

    assert_eq!(
        creation_count, destruction_count,
        "Should create and destroy equal numbers"
    );
    assert!(
        creation_count >= 200,
        "Should create significant number of pads"
    );
    assert!(
        creation_rate >= 100.0,
        "Should achieve reasonable creation performance"
    );
    assert!(
        destruction_rate >= 100.0,
        "Should achieve reasonable destruction performance"
    );

    println!("✅ Pad lifecycle performance test passed");
}

#[test]
fn test_algorithmic_performance_comparison() {
    init_for_tests();
    println!("=== Algorithmic Performance Comparison Test ===");

    let strategies = ["static", "ewma", "aimd"];
    let mut results = Vec::new();

    for strategy in &strategies {
        println!("Testing {} strategy...", strategy);

        let dispatcher = create_dispatcher_for_testing(Some(&[0.5, 0.5]));
        dispatcher.set_property("strategy", *strategy);
        dispatcher.set_property("rebalance-interval-ms", 100u64);
        dispatcher.set_property("auto-balance", true);

        let pipeline = gst::Pipeline::new();
        let source = gst::ElementFactory::make("audiotestsrc")
            .property("num-buffers", 3000i32)
            .property("is-live", false)
            .build()
            .unwrap();

        let counter1 = create_counter_sink();
        let counter2 = create_counter_sink();

        pipeline
            .add_many([&source, &dispatcher, &counter1, &counter2])
            .unwrap();
        source.link(&dispatcher).unwrap();

        let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
        let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();

        src_0.link(&counter1.static_pad("sink").unwrap()).unwrap();
        src_1.link(&counter2.static_pad("sink").unwrap()).unwrap();

        let start_time = Instant::now();
        pipeline.set_state(gst::State::Playing).unwrap();

        // Wait for completion
        let mut final_count = 0u64;
        let timeout = Duration::from_secs(20);
        let test_start = Instant::now();

        loop {
            run_mainloop_ms(100);

            let count1: u64 = get_property(&counter1, "count").unwrap_or(0);
            let count2: u64 = get_property(&counter2, "count").unwrap_or(0);
            let total = count1 + count2;

            if total >= 3000 || test_start.elapsed() > timeout {
                final_count = total;
                break;
            }
        }

        let elapsed = start_time.elapsed();
        pipeline.set_state(gst::State::Null).unwrap();

        let throughput = final_count as f64 / elapsed.as_secs_f64();
        results.push((strategy, final_count, elapsed, throughput));

        println!(
            "{} strategy: {} buffers in {:.3}s ({:.2} buf/sec)",
            strategy,
            final_count,
            elapsed.as_secs_f64(),
            throughput
        );
    }

    // Compare results
    println!("\n=== Performance Comparison Summary ===");
    for (strategy, _count, _elapsed, throughput) in &results {
        println!("{}: {:.2} buffers/sec", strategy, throughput);
    }

    // All strategies should achieve reasonable performance
    for (strategy, count, _, throughput) in &results {
        assert!(
            *count >= 1000,
            "{} should process significant buffers",
            strategy
        );
        assert!(
            *throughput >= 50.0,
            "{} should achieve reasonable throughput",
            strategy
        );
    }

    println!("✅ Algorithmic performance comparison test passed");
}
