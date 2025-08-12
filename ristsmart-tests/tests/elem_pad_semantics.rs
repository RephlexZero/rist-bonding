// Test pad semantics: Caps proxying, stickies replay, EOS/FLUSH fanout

use gst::prelude::*;
use gstreamer as gst;
use gstreamer_app as gst_app;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

/// Test caps proxying from sink to src pads
#[test]
fn test_caps_proxying() {
    ristsmart_tests::register_everything_for_tests();

    let pipeline = gst::Pipeline::new();
    let appsrc = gst::ElementFactory::make("appsrc")
        .property("format", &gst::Format::Time)
        .build()
        .expect("Failed to create appsrc");

    let dispatcher = gst::ElementFactory::make("ristdispatcher")
        .property("auto-balance", false)
        .build()
        .expect("Failed to create ristdispatcher");

    let fakesink1 = gst::ElementFactory::make("fakesink")
        .name("sink1")
        .build()
        .expect("Failed to create fakesink1");

    let fakesink2 = gst::ElementFactory::make("fakesink")
        .name("sink2")
        .build()
        .expect("Failed to create fakesink2");

    pipeline.add_many(&[&appsrc, &dispatcher, &fakesink1, &fakesink2]).unwrap();

    // Link appsrc to dispatcher
    appsrc.link(&dispatcher).expect("Failed to link appsrc to dispatcher");

    // Request src pads and link to sinks
    let src_pad1 = dispatcher.request_pad_simple("src_%u").expect("Failed to request src pad 1");
    let src_pad2 = dispatcher.request_pad_simple("src_%u").expect("Failed to request src pad 2");

    src_pad1.link(&fakesink1.static_pad("sink").unwrap()).expect("Failed to link to sink1");
    src_pad2.link(&fakesink2.static_pad("sink").unwrap()).expect("Failed to link to sink2");

    // Set specific caps on appsrc
    let test_caps = gst::Caps::builder("video/x-h264")
        .field("width", 1920i32)
        .field("height", 1080i32)
        .field("framerate", gst::Fraction::new(30, 1))
        .field("profile", "high")
        .build();

    appsrc.set_property("caps", &test_caps);

    pipeline.set_state(gst::State::Playing).expect("Failed to set pipeline to Playing");

    // Allow caps negotiation to complete
    std::thread::sleep(Duration::from_millis(100));

    // Check that src pads have the same caps as the sink pad
    let sink_pad = dispatcher.static_pad("sink").expect("Failed to get sink pad");
    let sink_caps = sink_pad.current_caps();
    
    let src1_caps = src_pad1.current_caps();
    let src2_caps = src_pad2.current_caps();

    println!("Sink caps: {:?}", sink_caps);
    println!("Src1 caps: {:?}", src1_caps);  
    println!("Src2 caps: {:?}", src2_caps);

    // Verify caps were propagated
    assert!(sink_caps.is_some(), "Sink pad should have negotiated caps");
    assert!(src1_caps.is_some(), "Src pad 1 should have negotiated caps");
    assert!(src2_caps.is_some(), "Src pad 2 should have negotiated caps");

    if let (Some(sink_caps), Some(src1_caps), Some(src2_caps)) = (sink_caps, src1_caps, src2_caps) {
        // Check that caps are equivalent  
        assert!(sink_caps == src1_caps, 
                "Src pad 1 caps should match sink caps");
        assert!(sink_caps == src2_caps, 
                "Src pad 2 caps should match sink caps");

        // Check specific fields are preserved
        let sink_struct = sink_caps.structure(0).expect("Sink caps should have structure");
        let src1_struct = src1_caps.structure(0).expect("Src1 caps should have structure");

        assert_eq!(sink_struct.get::<i32>("width"), src1_struct.get::<i32>("width"),
                   "Width should be preserved");
        assert_eq!(sink_struct.get::<i32>("height"), src1_struct.get::<i32>("height"), 
                   "Height should be preserved");
    }

    pipeline.set_state(gst::State::Null).expect("Failed to set pipeline to Null");
    println!("Caps proxying test passed!");
}

/// Test sticky events replay on new src pads
#[test]
fn test_sticky_events_replay() {
    ristsmart_tests::register_everything_for_tests();

    let pipeline = gst::Pipeline::new();
    let appsrc = gst::ElementFactory::make("appsrc")
        .property("caps", &gst::Caps::builder("video/x-raw").build())
        .property("format", &gst::Format::Time)
        .build()
        .expect("Failed to create appsrc");

    let dispatcher = gst::ElementFactory::make("ristdispatcher")
        .property("auto-balance", false)
        .build()
        .expect("Failed to create ristdispatcher");

    pipeline.add_many(&[&appsrc, &dispatcher]).unwrap();
    appsrc.link(&dispatcher).expect("Failed to link appsrc to dispatcher");

    // Start pipeline to establish sticky events
    pipeline.set_state(gst::State::Playing).expect("Failed to set pipeline to Playing");

    let appsrc = appsrc.dynamic_cast::<gst_app::AppSrc>().unwrap();

    // Push a buffer to ensure sticky events are established
    let data = vec![b'T'; 100];
    let mut buffer = gst::Buffer::with_size(data.len()).unwrap();
    {
        let buffer_ref = buffer.get_mut().unwrap();
        buffer_ref.set_pts(gst::ClockTime::from_mseconds(0));
    }
    appsrc.push_buffer(buffer).expect("Failed to push buffer");

    // Allow events to propagate
    std::thread::sleep(Duration::from_millis(100));

    // Create event monitoring sink
    let events_received = Arc::new(Mutex::new(Vec::new()));
    let events_clone = events_received.clone();

    let event_monitoring_sink = gst::ElementFactory::make("fakesink")
        .property("signal-handoffs", true)
        .build()
        .expect("Failed to create monitoring sink");

    // Monitor events on the sink pad
    let sink_pad = event_monitoring_sink.static_pad("sink").unwrap();
    sink_pad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_pad, info| {
        if let Some(gst::PadProbeData::Event(ref event)) = info.data {
            events_clone.lock().unwrap().push(event.type_());
        }
        gst::PadProbeReturn::Ok
    });

    // Now request a new src pad and link it (this should trigger sticky event replay)
    let src_pad = dispatcher.request_pad_simple("src_%u").expect("Failed to request src pad");
    
    pipeline.add(&event_monitoring_sink).unwrap();
    src_pad.link(&sink_pad).expect("Failed to link to monitoring sink");

    // Allow sticky events to be replayed
    std::thread::sleep(Duration::from_millis(100));

    let received_events = events_received.lock().unwrap();
    println!("Received events: {:?}", *received_events);

    // Verify that essential sticky events were replayed
    assert!(received_events.contains(&gst::EventType::StreamStart),
            "STREAM_START event should be replayed");
    assert!(received_events.contains(&gst::EventType::Caps),
            "CAPS event should be replayed");
    assert!(received_events.contains(&gst::EventType::Segment),
            "SEGMENT event should be replayed");

    // Events should be in the correct order
    let stream_start_pos = received_events.iter().position(|&x| x == gst::EventType::StreamStart);
    let caps_pos = received_events.iter().position(|&x| x == gst::EventType::Caps);
    let segment_pos = received_events.iter().position(|&x| x == gst::EventType::Segment);

    if let (Some(ss_pos), Some(caps_pos), Some(seg_pos)) = (stream_start_pos, caps_pos, segment_pos) {
        assert!(ss_pos < caps_pos, "STREAM_START should come before CAPS");
        assert!(caps_pos < seg_pos, "CAPS should come before SEGMENT");
    }

    pipeline.set_state(gst::State::Null).expect("Failed to set pipeline to Null");
    println!("Sticky events replay test passed!");
}

/// Test EOS fanout to all src pads
#[test]
fn test_eos_fanout() {
    ristsmart_tests::register_everything_for_tests();

    let pipeline = gst::Pipeline::new();
    let appsrc = gst::ElementFactory::make("appsrc")
        .property("caps", &gst::Caps::builder("application/x-rtp").build())
        .property("format", &gst::Format::Time)
        .build()
        .expect("Failed to create appsrc");

    let dispatcher = gst::ElementFactory::make("ristdispatcher")
        .property("auto-balance", false)
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

    let counter_sink3 = gst::ElementFactory::make("counter_sink")
        .name("counter3")
        .build()
        .expect("Failed to create counter_sink 3");

    pipeline.add_many(&[&appsrc, &dispatcher, &counter_sink1, &counter_sink2, &counter_sink3]).unwrap();

    // Link elements
    appsrc.link(&dispatcher).expect("Failed to link appsrc to dispatcher");

    let src_pad1 = dispatcher.request_pad_simple("src_%u").expect("Failed to request src pad 1");
    let src_pad2 = dispatcher.request_pad_simple("src_%u").expect("Failed to request src pad 2");
    let src_pad3 = dispatcher.request_pad_simple("src_%u").expect("Failed to request src pad 3");

    src_pad1.link(&counter_sink1.static_pad("sink").unwrap()).expect("Failed to link to counter1");
    src_pad2.link(&counter_sink2.static_pad("sink").unwrap()).expect("Failed to link to counter2");
    src_pad3.link(&counter_sink3.static_pad("sink").unwrap()).expect("Failed to link to counter3");

    pipeline.set_state(gst::State::Playing).expect("Failed to set pipeline to Playing");

    let appsrc = appsrc.dynamic_cast::<gst_app::AppSrc>().unwrap();

    // Push some buffers
    for i in 0..10 {
        let data = vec![b'E'; 100];
        let mut buffer = gst::Buffer::with_size(data.len()).unwrap();
        {
            let buffer_ref = buffer.get_mut().unwrap();
            buffer_ref.set_pts(gst::ClockTime::from_mseconds(i * 100));
        }
        appsrc.push_buffer(buffer).expect("Failed to push buffer");
    }

    // Send EOS
    appsrc.end_of_stream().expect("Failed to send EOS");

    // Wait for EOS propagation
    let bus = pipeline.bus().expect("Failed to get bus");
    let timeout = Some(gst::ClockTime::from_seconds(5));
    match bus.timed_pop_filtered(timeout, &[gst::MessageType::Eos, gst::MessageType::Error]) {
        Some(msg) => match msg.view() {
            gst::MessageView::Eos(..) => println!("EOS received at pipeline level"),
            gst::MessageView::Error(err) => {
                panic!("Pipeline error: {}", err.error());
            }
            _ => panic!("Unexpected message"),
        },
        None => panic!("Timeout waiting for EOS"),
    }

    // Check that all counter sinks received EOS
    let got_eos1: bool = counter_sink1.property("got-eos");
    let got_eos2: bool = counter_sink2.property("got-eos");  
    let got_eos3: bool = counter_sink3.property("got-eos");

    let count1: u64 = counter_sink1.property("count");
    let count2: u64 = counter_sink2.property("count");
    let count3: u64 = counter_sink3.property("count");

    println!("EOS received - counter1: {}, counter2: {}, counter3: {}", got_eos1, got_eos2, got_eos3);
    println!("Buffer counts - counter1: {}, counter2: {}, counter3: {}", count1, count2, count3);

    // Verify EOS fanout
    assert!(got_eos1, "Counter sink 1 should have received EOS");
    assert!(got_eos2, "Counter sink 2 should have received EOS");
    assert!(got_eos3, "Counter sink 3 should have received EOS");

    // Verify all buffers were distributed 
    assert_eq!(count1 + count2 + count3, 10, "All buffers should be accounted for");

    pipeline.set_state(gst::State::Null).expect("Failed to set pipeline to Null");
    println!("EOS fanout test passed!");
}

/// Test FLUSH events fanout
#[test]
fn test_flush_fanout() {
    ristsmart_tests::register_everything_for_tests();

    let pipeline = gst::Pipeline::new();
    let appsrc = gst::ElementFactory::make("appsrc")
        .property("caps", &gst::Caps::builder("application/x-rtp").build())
        .property("format", &gst::Format::Time)
        .build()
        .expect("Failed to create appsrc");

    let dispatcher = gst::ElementFactory::make("ristdispatcher")
        .property("auto-balance", false)
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

    pipeline.add_many(&[&appsrc, &dispatcher, &counter_sink1, &counter_sink2]).unwrap();

    // Link elements
    appsrc.link(&dispatcher).expect("Failed to link appsrc to dispatcher");

    let src_pad1 = dispatcher.request_pad_simple("src_%u").expect("Failed to request src pad 1");
    let src_pad2 = dispatcher.request_pad_simple("src_%u").expect("Failed to request src pad 2");

    src_pad1.link(&counter_sink1.static_pad("sink").unwrap()).expect("Failed to link to counter1");
    src_pad2.link(&counter_sink2.static_pad("sink").unwrap()).expect("Failed to link to counter2");

    pipeline.set_state(gst::State::Playing).expect("Failed to set pipeline to Playing");

    let appsrc = appsrc.dynamic_cast::<gst_app::AppSrc>().unwrap();

    // Push some buffers
    for i in 0..5 {
        let data = vec![b'F'; 100];
        let mut buffer = gst::Buffer::with_size(data.len()).unwrap();
        {
            let buffer_ref = buffer.get_mut().unwrap();
            buffer_ref.set_pts(gst::ClockTime::from_mseconds(i * 100));
        }
        appsrc.push_buffer(buffer).expect("Failed to push buffer");
    }

    std::thread::sleep(Duration::from_millis(100));

    // Perform seek to trigger flush events
    pipeline.seek_simple(
        gst::SeekFlags::FLUSH, 
        gst::ClockTime::from_seconds(0)
    ).expect("Seek should succeed");

    std::thread::sleep(Duration::from_millis(100));

    // Check that flush events were received by all sinks
    let got_flush_start1: bool = counter_sink1.property("got-flush-start");
    let got_flush_start2: bool = counter_sink2.property("got-flush-start");
    let got_flush_stop1: bool = counter_sink1.property("got-flush-stop");
    let got_flush_stop2: bool = counter_sink2.property("got-flush-stop");

    println!("Flush start - counter1: {}, counter2: {}", got_flush_start1, got_flush_start2);
    println!("Flush stop - counter1: {}, counter2: {}", got_flush_stop1, got_flush_stop2);

    // Verify flush fanout 
    assert!(got_flush_start1, "Counter sink 1 should have received FLUSH_START");
    assert!(got_flush_start2, "Counter sink 2 should have received FLUSH_START");
    assert!(got_flush_stop1, "Counter sink 1 should have received FLUSH_STOP");
    assert!(got_flush_stop2, "Counter sink 2 should have received FLUSH_STOP");

    // Clean shutdown
    appsrc.end_of_stream().expect("Failed to send EOS");

    let bus = pipeline.bus().expect("Failed to get bus");
    let timeout = Some(gst::ClockTime::from_seconds(5));
    bus.timed_pop_filtered(timeout, &[gst::MessageType::Eos, gst::MessageType::Error]);

    pipeline.set_state(gst::State::Null).expect("Failed to set pipeline to Null");
    println!("FLUSH fanout test passed!");
}
