/// Simple test to verify video generation without bonding
use std::thread;
use std::time::Duration;
use gstreamer as gst;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    gst::init()?;
    
    println!("Creating simple video test pipeline...");
    
    // Simple pipeline: videotestsrc -> x265enc -> h265parse -> mpegtsmux -> filesink
    let pipeline = gst::Pipeline::new();
    
    let videotestsrc = gst::ElementFactory::make("videotestsrc")
        .property("is-live", false)  // Not live for simpler testing
        .property_from_str("pattern", "smpte")
        .property("num-buffers", 300i32)  // 5 seconds at 60fps
        .build()?;
        
    let videoconvert = gst::ElementFactory::make("videoconvert").build()?;
    
    let video_caps = gst::Caps::builder("video/x-raw")
        .field("format", "I420")
        .field("width", 640i32)  // Lower resolution for testing
        .field("height", 480i32)
        .field("framerate", gst::Fraction::new(30, 1))  // Lower framerate
        .build();
    let videocaps = gst::ElementFactory::make("capsfilter")
        .property("caps", &video_caps)
        .build()?;
        
    let x265enc = gst::ElementFactory::make("x265enc")
        .property("bitrate", 1000u32)  // Lower bitrate
        .property_from_str("tune", "zerolatency")
        .property_from_str("speed-preset", "superfast")
        .property_from_str("option-string", "pools=none")
        .build()?;
        
    let h265parse = gst::ElementFactory::make("h265parse").build()?;
    let mpegtsmux = gst::ElementFactory::make("mpegtsmux").build()?;
    let filesink = gst::ElementFactory::make("filesink")
        .property("location", "/workspace/target/simple_test.ts")
        .build()?;
    
    pipeline.add_many([&videotestsrc, &videoconvert, &videocaps, &x265enc, &h265parse, &mpegtsmux, &filesink])?;
    
    // Link everything
    gst::Element::link_many([&videotestsrc, &videoconvert, &videocaps, &x265enc, &h265parse, &mpegtsmux, &filesink])?;
    
    println!("Starting simple pipeline...");
    pipeline.set_state(gst::State::Playing)?;
    
    let bus = pipeline.bus().unwrap();
    
    loop {
        match bus.timed_pop_filtered(gst::ClockTime::from_seconds(1), &[
            gst::MessageType::Error,
            gst::MessageType::Eos,
            gst::MessageType::StateChanged,
        ]) {
            Some(msg) => {
                match msg.view() {
                    gst::MessageView::Error(err) => {
                        eprintln!("Error: {} - {}", err.error(), err.debug().unwrap_or_default());
                        break;
                    }
                    gst::MessageView::Eos(_) => {
                        println!("End of stream reached");
                        break;
                    }
                    gst::MessageView::StateChanged(state_changed) => {
                        if state_changed.src() == Some(pipeline.upcast_ref()) {
                            println!("Pipeline state: {:?} -> {:?}", 
                                   state_changed.old(), state_changed.current());
                        }
                    }
                    _ => {}
                }
            }
            None => {
                // Check file size
                if let Ok(metadata) = std::fs::metadata("/workspace/target/simple_test.ts") {
                    println!("File size: {}KB", metadata.len() / 1024);
                }
            }
        }
    }
    
    pipeline.set_state(gst::State::Null)?;
    println!("Simple test completed!");
    
    Ok(())
}
