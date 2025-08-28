use gst::prelude::*;
use gstreamer as gst;
use gstristelements::testing::*;
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::time::Duration;

/// Pump the GLib main loop for the specified duration
fn run_mainloop_ms(ms: u64) {
    let ctx = glib::MainContext::default();
    let _guard = ctx.acquire().expect("acquire main context");
    let end = std::time::Instant::now() + Duration::from_millis(ms);
    while std::time::Instant::now() < end {
        while ctx.iteration(false) {}
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn test_concurrent_weight_updates() {
    init_for_tests();
    println!("=== Concurrent Weight Updates Test ===");

    let dispatcher = create_dispatcher_for_testing(Some(&[0.5, 0.3, 0.2]));
    let num_threads = 4;
    let barrier = Arc::new(Barrier::new(num_threads));
    let success_count = Arc::new(Mutex::new(0));

    // Spawn multiple threads that concurrently update weights
    let handles: Vec<_> = (0..num_threads)
        .map(|i| {
            let dispatcher = dispatcher.clone();
            let barrier = barrier.clone();
            let success_count = success_count.clone();

            thread::spawn(move || {
                // Wait for all threads to be ready
                barrier.wait();

                // Each thread tries different weight combinations
                let weight_sets = vec![
                    "[0.4, 0.3, 0.3]",
                    "[0.6, 0.2, 0.2]",
                    "[0.33, 0.33, 0.34]",
                    "[0.7, 0.15, 0.15]",
                ];

                let weights = weight_sets[i % weight_sets.len()];

                // Perform multiple updates to stress test
                for _ in 0..10 {
                    dispatcher.set_property("weights", weights);
                    thread::sleep(Duration::from_millis(1));
                }

                // Verify the property can still be read (tests internal consistency)
                let final_weights: String = dispatcher.property("weights");
                if !final_weights.is_empty() {
                    let mut count = success_count.lock().unwrap();
                    *count += 1;
                }
            })
        })
        .collect();

    // Wait for all threads to complete
    for handle in handles {
        handle.join().expect("Thread should complete");
    }

    let final_count = *success_count.lock().unwrap();
    assert_eq!(
        final_count, num_threads,
        "All threads should successfully update weights"
    );

    // Verify dispatcher is still functional
    let final_weights: String = dispatcher.property("weights");
    assert!(
        !final_weights.is_empty(),
        "Dispatcher should still be functional"
    );

    println!(
        "✅ Concurrent weight updates test passed - {} threads completed",
        num_threads
    );
}

#[test]
fn test_concurrent_pad_management() {
    init_for_tests();
    println!("=== Concurrent Pad Management Test ===");

    let pipeline = gst::Pipeline::new();
    let dispatcher = create_dispatcher_for_testing(Some(&[1.0]));
    let source = create_test_source();

    pipeline.add_many([&source, &dispatcher]).unwrap();
    source.link(&dispatcher).unwrap();

    let num_threads = 3;
    let barrier = Arc::new(Barrier::new(num_threads));
    let operations_completed = Arc::new(Mutex::new(0));

    pipeline.set_state(gst::State::Playing).unwrap();
    thread::sleep(Duration::from_millis(100)); // Let pipeline start

    // Each thread manages its own pads concurrently
    let handles: Vec<_> = (0..num_threads)
        .map(|i| {
            let dispatcher = dispatcher.clone();
            let pipeline = pipeline.clone();
            let barrier = barrier.clone();
            let operations_completed = operations_completed.clone();

            thread::spawn(move || {
                barrier.wait();

                // Each thread creates and manages its own counter
                let counter = create_counter_sink();
                pipeline.add(&counter).unwrap();

                // Request a pad and link it
                let src_pad = dispatcher.request_pad_simple("src_%u");
                if let Some(pad) = src_pad {
                    if pad.link(&counter.static_pad("sink").unwrap()).is_ok() {
                        // Let it run briefly
                        thread::sleep(Duration::from_millis(50));

                        // Clean up
                        let _ = pad.unlink(&counter.static_pad("sink").unwrap());
                        pipeline.remove(&counter).ok();
                        dispatcher.release_request_pad(&pad);

                        let mut count = operations_completed.lock().unwrap();
                        *count += 1;
                    }
                }
            })
        })
        .collect();

    // Wait for all threads
    for handle in handles {
        handle.join().expect("Thread should complete");
    }

    let completed = *operations_completed.lock().unwrap();
    pipeline.set_state(gst::State::Null).unwrap();

    assert_eq!(
        completed, num_threads,
        "All concurrent pad operations should succeed"
    );
    println!(
        "✅ Concurrent pad management test passed - {} operations completed",
        completed
    );
}

#[test]
fn test_concurrent_property_access() {
    init_for_tests();
    println!("=== Concurrent Property Access Test ===");

    let dispatcher = create_dispatcher_for_testing(Some(&[0.4, 0.6]));
    let num_readers = 5;
    let num_writers = 2;
    let barrier = Arc::new(Barrier::new(num_readers + num_writers));
    let read_successes = Arc::new(Mutex::new(0));
    let write_successes = Arc::new(Mutex::new(0));

    // Reader threads
    let reader_handles: Vec<_> = (0..num_readers)
        .map(|_| {
            let dispatcher = dispatcher.clone();
            let barrier = barrier.clone();
            let read_successes = read_successes.clone();

            thread::spawn(move || {
                barrier.wait();

                // Continuously read properties for a brief period
                let end_time = std::time::Instant::now() + Duration::from_millis(200);
                while std::time::Instant::now() < end_time {
                    let _weights: String = dispatcher.property("weights");
                    let _strategy: String = dispatcher.property("strategy");
                    let _interval: u64 = dispatcher.property("rebalance-interval-ms");
                    thread::sleep(Duration::from_micros(100));
                }

                let mut count = read_successes.lock().unwrap();
                *count += 1;
            })
        })
        .collect();

    // Writer threads
    let writer_handles: Vec<_> = (0..num_writers)
        .map(|i| {
            let dispatcher = dispatcher.clone();
            let barrier = barrier.clone();
            let write_successes = write_successes.clone();

            thread::spawn(move || {
                barrier.wait();

                // Alternate between different property updates
                let weight_sets = ["[0.3, 0.7]", "[0.8, 0.2]"];

                for j in 0..20 {
                    let weights = weight_sets[(i + j) % weight_sets.len()];
                    dispatcher.set_property("weights", weights);
                    dispatcher.set_property("rebalance-interval-ms", 300u64 + (j * 50) as u64);
                    thread::sleep(Duration::from_millis(5));
                }

                let mut count = write_successes.lock().unwrap();
                *count += 1;
            })
        })
        .collect();

    // Wait for all threads
    for handle in reader_handles {
        handle.join().expect("Reader thread should complete");
    }
    for handle in writer_handles {
        handle.join().expect("Writer thread should complete");
    }

    let reads = *read_successes.lock().unwrap();
    let writes = *write_successes.lock().unwrap();

    assert_eq!(reads, num_readers, "All reader threads should complete");
    assert_eq!(writes, num_writers, "All writer threads should complete");

    // Verify final state is still consistent
    let final_weights: String = dispatcher.property("weights");
    assert!(!final_weights.is_empty(), "Final weights should be valid");

    println!(
        "✅ Concurrent property access test passed - {} readers, {} writers",
        reads, writes
    );
}

#[test]
fn test_pipeline_state_transitions_concurrent() {
    init_for_tests();
    println!("=== Concurrent Pipeline State Transitions Test ===");

    let pipeline = gst::Pipeline::new();
    let dispatcher = create_dispatcher_for_testing(Some(&[0.5, 0.5]));
    let source = create_test_source();
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

    let num_threads = 3;
    let barrier = Arc::new(Barrier::new(num_threads));
    let state_changes = Arc::new(Mutex::new(0));

    // Concurrent state transitions and property updates
    let handles: Vec<_> = (0..num_threads)
        .map(|i| {
            let pipeline = pipeline.clone();
            let dispatcher = dispatcher.clone();
            let barrier = barrier.clone();
            let state_changes = state_changes.clone();

            thread::spawn(move || {
                barrier.wait();

                match i {
                    0 => {
                        // Thread 0: State transitions
                        for _ in 0..3 {
                            pipeline.set_state(gst::State::Playing).ok();
                            thread::sleep(Duration::from_millis(50));
                            pipeline.set_state(gst::State::Paused).ok();
                            thread::sleep(Duration::from_millis(50));
                        }
                    }
                    1 => {
                        // Thread 1: Weight updates during state changes
                        let weights = ["[0.6, 0.4]", "[0.4, 0.6]", "[0.7, 0.3]"];
                        for (j, weight) in weights.iter().enumerate() {
                            dispatcher.set_property("weights", *weight);
                            thread::sleep(Duration::from_millis(30));
                        }
                    }
                    2 => {
                        // Thread 2: Property reads during transitions
                        for _ in 0..10 {
                            let _weights: String = dispatcher.property("weights");
                            let _strategy: String = dispatcher.property("strategy");
                            thread::sleep(Duration::from_millis(15));
                        }
                    }
                    _ => unreachable!(),
                }

                let mut count = state_changes.lock().unwrap();
                *count += 1;
            })
        })
        .collect();

    // Wait for all concurrent operations
    for handle in handles {
        handle.join().expect("Thread should complete");
    }

    let completed = *state_changes.lock().unwrap();

    // Ensure pipeline ends in a clean state
    pipeline.set_state(gst::State::Null).unwrap();

    assert_eq!(
        completed, num_threads,
        "All concurrent operations should complete"
    );

    // Verify dispatcher is still functional
    let final_weights: String = dispatcher.property("weights");
    assert!(
        !final_weights.is_empty(),
        "Dispatcher should remain functional"
    );

    println!("✅ Concurrent pipeline state transitions test passed");
}

#[test]
fn test_stress_weight_updates_high_frequency() {
    init_for_tests();
    println!("=== High Frequency Weight Updates Stress Test ===");

    let dispatcher = create_dispatcher_for_testing(Some(&[0.33, 0.33, 0.34]));
    let num_threads = 6;
    let updates_per_thread = 100;
    let barrier = Arc::new(Barrier::new(num_threads));
    let total_updates = Arc::new(Mutex::new(0));

    let handles: Vec<_> = (0..num_threads)
        .map(|i| {
            let dispatcher = dispatcher.clone();
            let barrier = barrier.clone();
            let total_updates = total_updates.clone();

            thread::spawn(move || {
                barrier.wait();

                let weight_patterns = [
                    "[1.0, 0.0, 0.0]",
                    "[0.0, 1.0, 0.0]",
                    "[0.0, 0.0, 1.0]",
                    "[0.5, 0.5, 0.0]",
                    "[0.33, 0.33, 0.34]",
                    "[0.6, 0.2, 0.2]",
                ];

                for j in 0..updates_per_thread {
                    let pattern = weight_patterns[(i + j) % weight_patterns.len()];
                    dispatcher.set_property("weights", pattern);

                    // Very brief sleep to allow some interleaving
                    if j % 10 == 0 {
                        thread::sleep(Duration::from_micros(100));
                    }
                }

                let mut count = total_updates.lock().unwrap();
                *count += updates_per_thread;
            })
        })
        .collect();

    let start_time = std::time::Instant::now();

    // Wait for all threads
    for handle in handles {
        handle.join().expect("Thread should complete");
    }

    let duration = start_time.elapsed();
    let completed_updates = *total_updates.lock().unwrap();
    let expected_updates = num_threads * updates_per_thread;

    assert_eq!(
        completed_updates, expected_updates,
        "All updates should complete"
    );

    // Verify dispatcher integrity after stress test
    let final_weights: String = dispatcher.property("weights");
    assert!(
        !final_weights.is_empty(),
        "Dispatcher should remain functional"
    );

    // Verify we can still set and read weights normally
    dispatcher.set_property("weights", "[0.25, 0.25, 0.5]");
    let verification_weights: String = dispatcher.property("weights");
    assert!(
        verification_weights.contains("0.25"),
        "Should be able to set weights after stress test"
    );

    println!(
        "✅ High frequency stress test passed - {} updates in {:.2}s ({:.0} updates/sec)",
        completed_updates,
        duration.as_secs_f64(),
        completed_updates as f64 / duration.as_secs_f64()
    );
}

#[test]
fn test_memory_safety_under_concurrent_access() {
    init_for_tests();
    println!("=== Memory Safety Under Concurrent Access Test ===");

    let dispatcher = create_dispatcher_for_testing(Some(&[0.4, 0.6]));
    let pipeline = gst::Pipeline::new();
    let source = create_test_source();

    pipeline.add_many([&source, &dispatcher]).unwrap();
    source.link(&dispatcher).unwrap();

    let num_threads = 4;
    let barrier = Arc::new(Barrier::new(num_threads));
    let operations_completed = Arc::new(Mutex::new(0));

    pipeline.set_state(gst::State::Playing).unwrap();

    let handles: Vec<_> = (0..num_threads)
        .map(|i| {
            let dispatcher = dispatcher.clone();
            let pipeline = pipeline.clone();
            let barrier = barrier.clone();
            let operations_completed = operations_completed.clone();

            thread::spawn(move || {
                barrier.wait();

                // Each thread performs different types of operations
                match i % 4 {
                    0 => {
                        // Pad management
                        for _ in 0..10 {
                            let counter = create_counter_sink();
                            pipeline.add(&counter).ok();

                            if let Some(pad) = dispatcher.request_pad_simple("src_%u") {
                                let _ = pad.link(&counter.static_pad("sink").unwrap());
                                thread::sleep(Duration::from_millis(5));
                                let _ = pad.unlink(&counter.static_pad("sink").unwrap());
                                dispatcher.release_request_pad(&pad);
                            }

                            pipeline.remove(&counter).ok();
                        }
                    }
                    1 => {
                        // Weight updates
                        let weights = ["[0.3, 0.7]", "[0.8, 0.2]", "[0.5, 0.5]"];
                        for weight in weights.iter().cycle().take(30) {
                            dispatcher.set_property("weights", *weight);
                            thread::sleep(Duration::from_millis(2));
                        }
                    }
                    2 => {
                        // Property reads
                        for _ in 0..50 {
                            let _weights: String = dispatcher.property("weights");
                            let _current_weights: String = dispatcher.property("current-weights");
                            let _strategy: String = dispatcher.property("strategy");
                            thread::sleep(Duration::from_millis(1));
                        }
                    }
                    3 => {
                        // Mixed operations
                        for j in 0..20 {
                            if j % 2 == 0 {
                                dispatcher
                                    .set_property("rebalance-interval-ms", (200 + j * 10) as u64);
                            } else {
                                let _interval: u64 = dispatcher.property("rebalance-interval-ms");
                            }
                            thread::sleep(Duration::from_millis(3));
                        }
                    }
                    _ => unreachable!(),
                }

                let mut count = operations_completed.lock().unwrap();
                *count += 1;
            })
        })
        .collect();

    // Let threads run concurrently
    for handle in handles {
        handle
            .join()
            .expect("Thread should complete without panicking");
    }

    let completed = *operations_completed.lock().unwrap();
    pipeline.set_state(gst::State::Null).unwrap();

    assert_eq!(completed, num_threads, "All threads should complete safely");

    // Final verification that dispatcher is still functional
    dispatcher.set_property("weights", "[0.1, 0.9]");
    let final_check: String = dispatcher.property("weights");
    assert!(
        final_check.contains("0.1"),
        "Dispatcher should remain functional after concurrent stress"
    );

    println!(
        "✅ Memory safety test passed - {} concurrent operations completed safely",
        completed
    );
}
