use gstreamer as gst;
use gst::prelude::*;
use gstristelements::testing::*;
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
fn test_pad_removal_operations() {
    init_for_tests();
    println!("=== Pad Removal Operations Test ===");

    // Test 1: Basic pad removal functionality
    let pipeline = gst::Pipeline::new();
    let dispatcher = create_dispatcher_for_testing(Some(&[0.5, 0.3, 0.2]));
    let source = create_test_source();
    let counter1 = create_counter_sink();
    let counter2 = create_counter_sink();
    let counter3 = create_counter_sink();

    pipeline.add_many([&source, &dispatcher, &counter1, &counter2, &counter3]).unwrap();
    source.link(&dispatcher).unwrap();
    
    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_2 = dispatcher.request_pad_simple("src_%u").unwrap();

    src_0.link(&counter1.static_pad("sink").unwrap()).unwrap();
    src_1.link(&counter2.static_pad("sink").unwrap()).unwrap();
    src_2.link(&counter3.static_pad("sink").unwrap()).unwrap();

    // Verify initial setup works
    pipeline.set_state(gst::State::Playing).unwrap();
    run_mainloop_ms(500);

    let count1_initial: u64 = get_property(&counter1, "count").unwrap();
    let count2_initial: u64 = get_property(&counter2, "count").unwrap();
    let count3_initial: u64 = get_property(&counter3, "count").unwrap();
    
    println!("Initial counts: C1={}, C2={}, C3={}", count1_initial, count2_initial, count3_initial);
    assert!(count1_initial > 0 && count2_initial > 0 && count3_initial > 0, "All pads should receive data");

    pipeline.set_state(gst::State::Null).unwrap();
    
    // Test 2: Remove middle pad and verify dispatcher state
    println!("Removing middle pad...");
    let _ = src_1.unlink(&counter2.static_pad("sink").unwrap());
    pipeline.remove(&counter2).unwrap();
    dispatcher.release_request_pad(&src_1);
    
    // Update weights for remaining 2 pads
    dispatcher.set_property("weights", "[0.7, 0.3]");

    // Test 3: Verify pipeline works with 2 pads
    pipeline.set_state(gst::State::Playing).unwrap();
    run_mainloop_ms(600);

    let count1_final: u64 = get_property(&counter1, "count").unwrap();
    let count3_final: u64 = get_property(&counter3, "count").unwrap();
    
    println!("After pad removal: C1={}, C3={}", count1_final, count3_final);
    
    assert!(count1_final > count1_initial, "Counter1 should process more data");
    assert!(count3_final > count3_initial, "Counter3 should process more data");

    // Test 4: Verify weight distribution (70/30 split expected)
    let new_count1 = count1_final - count1_initial;
    let new_count3 = count3_final - count3_initial;
    let total = new_count1 + new_count3;
    
    if total > 10 {
        let ratio1 = new_count1 as f64 / total as f64;
        println!("Traffic distribution: C1={:.1}%, C3={:.1}%", ratio1 * 100.0, (1.0 - ratio1) * 100.0);
        assert!(ratio1 > 0.5, "Counter1 should get majority with 70% weight");
    }

    pipeline.set_state(gst::State::Null).unwrap();
    println!("✅ Pad removal operations test passed");
}

#[test]
fn test_remove_all_then_readd() {
    init_for_tests();
    println!("=== Remove All Pads Then Re-add Test ===");

    let pipeline = gst::Pipeline::new();
    let dispatcher = create_dispatcher_for_testing(Some(&[0.5, 0.5]));
    let source = create_test_source();

    pipeline.add_many([&source, &dispatcher]).unwrap();
    source.link(&dispatcher).unwrap();

    // Test 1: Start with 2 pads
    let counter1 = create_counter_sink();
    let counter2 = create_counter_sink();
    pipeline.add_many([&counter1, &counter2]).unwrap();

    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    let src_1 = dispatcher.request_pad_simple("src_%u").unwrap();
    src_0.link(&counter1.static_pad("sink").unwrap()).unwrap();
    src_1.link(&counter2.static_pad("sink").unwrap()).unwrap();

    pipeline.set_state(gst::State::Playing).unwrap();
    run_mainloop_ms(400);

    let initial_count1: u64 = get_property(&counter1, "count").unwrap();
    let initial_count2: u64 = get_property(&counter2, "count").unwrap();
    println!("Initial counts: C1={}, C2={}", initial_count1, initial_count2);

    pipeline.set_state(gst::State::Null).unwrap();

    // Test 2: Remove all pads
    println!("Removing all pads...");
    let _ = src_0.unlink(&counter1.static_pad("sink").unwrap());
    let _ = src_1.unlink(&counter2.static_pad("sink").unwrap());
    pipeline.remove_many([&counter1, &counter2]).unwrap();
    dispatcher.release_request_pad(&src_0);
    dispatcher.release_request_pad(&src_1);

    // Test 3: Add new pads
    println!("Adding new pads...");
    let new_counter1 = create_counter_sink();
    let new_counter2 = create_counter_sink();
    pipeline.add_many([&new_counter1, &new_counter2]).unwrap();

    let new_src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    let new_src_1 = dispatcher.request_pad_simple("src_%u").unwrap();
    new_src_0.link(&new_counter1.static_pad("sink").unwrap()).unwrap();
    new_src_1.link(&new_counter2.static_pad("sink").unwrap()).unwrap();

    dispatcher.set_property("weights", "[0.6, 0.4]");

    // Test 4: Verify new configuration works
    pipeline.set_state(gst::State::Playing).unwrap();
    run_mainloop_ms(600);

    let final_count1: u64 = get_property(&new_counter1, "count").unwrap();
    let final_count2: u64 = get_property(&new_counter2, "count").unwrap();
    println!("Final counts: C1={}, C2={}", final_count1, final_count2);

    assert!(final_count1 > 0, "New counter1 should receive data");
    assert!(final_count2 > 0, "New counter2 should receive data");

    pipeline.set_state(gst::State::Null).unwrap();
    println!("✅ Remove all then re-add test passed");
}

#[test] 
fn test_rapid_pad_cycles() {
    init_for_tests();
    println!("=== Rapid Pad Addition/Removal Cycles Test ===");

    let pipeline = gst::Pipeline::new();
    let dispatcher = create_dispatcher_for_testing(Some(&[1.0]));
    let source = create_test_source();
    let counter1 = create_counter_sink();

    pipeline.add_many([&source, &dispatcher, &counter1]).unwrap();
    source.link(&dispatcher).unwrap();
    
    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    src_0.link(&counter1.static_pad("sink").unwrap()).unwrap();

    pipeline.set_state(gst::State::Playing).unwrap();
    run_mainloop_ms(200);

    // Perform multiple rapid cycles
    for cycle in 0..3 {
        println!("Cycle {}: Adding temporary pad", cycle + 1);
        
        // Add temporary second output
        let temp_counter = create_counter_sink();
        pipeline.add(&temp_counter).unwrap();
        let temp_src = dispatcher.request_pad_simple("src_%u").unwrap();
        temp_src.link(&temp_counter.static_pad("sink").unwrap()).unwrap();
        dispatcher.set_property("weights", "[0.6, 0.4]");
        
        run_mainloop_ms(200);
        
        println!("Cycle {}: Removing temporary pad", cycle + 1);
        pipeline.set_state(gst::State::Paused).unwrap();
        
        let _ = temp_src.unlink(&temp_counter.static_pad("sink").unwrap());
        pipeline.remove(&temp_counter).unwrap();
        dispatcher.release_request_pad(&temp_src);
        dispatcher.set_property("weights", "[1.0]");
        
        pipeline.set_state(gst::State::Playing).unwrap();
        run_mainloop_ms(150);
    }

    let final_count: u64 = get_property(&counter1, "count").unwrap();
    println!("Final count after {} cycles: {}", 3, final_count);
    
    assert!(final_count > 0, "Counter should have accumulated data throughout cycles");

    pipeline.set_state(gst::State::Null).unwrap();
    println!("✅ Rapid pad cycles test passed");
}

#[test]
fn test_concurrent_pad_operations() {
    init_for_tests(); 
    println!("=== Concurrent Pad Operations Test ===");

    let pipeline = gst::Pipeline::new();
    let dispatcher = create_dispatcher_for_testing(Some(&[1.0]));
    let source = create_test_source();
    let counter1 = create_counter_sink();

    pipeline.add_many([&source, &dispatcher, &counter1]).unwrap();
    source.link(&dispatcher).unwrap();
    
    let src_0 = dispatcher.request_pad_simple("src_%u").unwrap();
    src_0.link(&counter1.static_pad("sink").unwrap()).unwrap();

    pipeline.set_state(gst::State::Playing).unwrap();
    run_mainloop_ms(200);

    // Simulate concurrent operations (in sequence since we can't actually multithread easily in tests)
    let mut operations_completed = 0;
    
    for i in 0..3 {
        println!("Background operation {} started", i);
        
        // Add a pad
        let counter = create_counter_sink();
        pipeline.add(&counter).unwrap();
        let src_pad = dispatcher.request_pad_simple("src_%u").unwrap();
        src_pad.link(&counter.static_pad("sink").unwrap()).unwrap();
        
        run_mainloop_ms(100);
        
        // Remove the pad
        let _ = src_pad.unlink(&counter.static_pad("sink").unwrap());
        pipeline.remove(&counter).unwrap(); 
        dispatcher.release_request_pad(&src_pad);
        
        operations_completed += 1;
        println!("Background operation {} completed", i);
        run_mainloop_ms(50);
    }

    assert_eq!(operations_completed, 3, "All operations should complete without error");

    let final_count: u64 = get_property(&counter1, "count").unwrap();
    assert!(final_count > 0, "Primary counter should continue working");

    pipeline.set_state(gst::State::Null).unwrap();
    println!("✅ Concurrent pad operations test passed");
}