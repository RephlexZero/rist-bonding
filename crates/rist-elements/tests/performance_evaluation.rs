//! Performance evaluation test for RIST bonding
//!
//! This test creates a comprehensive#[derive(Clone, Debug)]
struct PerformanceStats {
    timestamp: Duration,
    configured_weights: Vec<f64>,
    actual_distributions: Vec<u64>,
    packet_loss: Vec<f64>,
}

use gst::prelude::*;
use gstreamer as gst;
use gstristelements::testing::*;
use plotters::prelude::*;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Network parameters for 4G/5G-like connections with time-varying bandwidth
#[derive(Clone, Debug)]
struct ConnectionProfile {
    name: String,
    base_bitrate_kbps: u32,
    max_bitrate_kbps: u32,
    min_bitrate_kbps: u32,
    base_delay_ms: u32,
    loss_percent: f64,
    bandwidth_variation_period_s: f64,
}

impl ConnectionProfile {
    fn connection_4g_good() -> Self {
        Self {
            name: "4G-Good".to_string(),
            base_bitrate_kbps: 2000,
            max_bitrate_kbps: 3000,
            min_bitrate_kbps: 1500,
            base_delay_ms: 25,
            loss_percent: 0.1,
            bandwidth_variation_period_s: 5.0,
        }
    }

    fn connection_4g_typical() -> Self {
        Self {
            name: "4G-Typical".to_string(),
            base_bitrate_kbps: 1200,
            max_bitrate_kbps: 1800,
            min_bitrate_kbps: 800,
            base_delay_ms: 40,
            loss_percent: 0.5,
            bandwidth_variation_period_s: 8.0,
        }
    }

    fn connection_5g_good() -> Self {
        Self {
            name: "5G-Good".to_string(),
            base_bitrate_kbps: 4000,
            max_bitrate_kbps: 6000,
            min_bitrate_kbps: 3000,
            base_delay_ms: 15,
            loss_percent: 0.05,
            bandwidth_variation_period_s: 3.0,
        }
    }

    fn connection_5g_poor() -> Self {
        Self {
            name: "5G-Poor".to_string(),
            base_bitrate_kbps: 800,
            max_bitrate_kbps: 1500,
            min_bitrate_kbps: 400,
            base_delay_ms: 60,
            loss_percent: 1.0,
            bandwidth_variation_period_s: 12.0,
        }
    }

}

/// Performance test collector
struct PerformanceCollector {
    stats: Arc<Mutex<Vec<PerformanceStats>>>,
    start_time: Instant,
    connections: Vec<ConnectionProfile>,
}

impl PerformanceCollector {
    fn new(connections: Vec<ConnectionProfile>) -> Self {
        Self {
            stats: Arc::new(Mutex::new(Vec::new())),
            start_time: Instant::now(),
            connections,
        }
    }

    fn collect_stats(
        &self,
        dispatcher: &gst::Element,
        counters: &[gst::Element],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let timestamp = self.start_time.elapsed();
        
        // Get configured weights from dispatcher
        let weights_json: String = gstristelements::testing::get_property(dispatcher, "weights").unwrap_or_else(|_| "[]".to_string());
        let configured_weights: Vec<f64> = serde_json::from_str(&weights_json).unwrap_or_default();
        
        // Get actual packet counts from counters
        let mut actual_distributions = Vec::new();
        for counter in counters {
            let count: u64 = gstristelements::testing::get_property(counter, "count").unwrap_or(0);
            actual_distributions.push(count);
        }
        
        // Calculate current bitrates based on elapsed time and network stats
        let packet_loss: Vec<f64> = self.connections.iter().map(|c| c.loss_percent).collect();
        
        let stats = PerformanceStats {
            timestamp,
            configured_weights,
            actual_distributions,
            packet_loss,
        };
        
        self.stats.lock().unwrap().push(stats);
        Ok(())
    }

    fn generate_svg_plots(&self, output_path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let stats = self.stats.lock().unwrap();
        if stats.is_empty() {
            return Err("No statistics collected".into());
        }

        // Create SVG backend
        let root = SVGBackend::new(output_path, (1200, 800)).into_drawing_area();
        root.fill(&WHITE)?;

        // Create two chart areas
        let areas = root.split_evenly((2, 1));
        let upper = &areas[0];
        let lower = &areas[1];

        // Plot 1: Weight distributions over time
        self.plot_weight_distributions(upper, &stats)?;
        
        // Plot 2: Network performance metrics
        self.plot_network_metrics(lower, &stats)?;

        root.present()?;
        println!("Performance plots saved to: {}", output_path);
        Ok(())
    }

    fn plot_weight_distributions(
        &self,
        area: &DrawingArea<SVGBackend, plotters::coord::Shift>,
        stats: &[PerformanceStats],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut chart = ChartBuilder::on(area)
            .caption("Weight Distributions Over Time", ("sans-serif", 30))
            .margin(10)
            .x_label_area_size(40)
            .y_label_area_size(50)
            .build_cartesian_2d(
                0f64..stats.last().unwrap().timestamp.as_secs_f64(),
                0f64..1.0f64,
            )?;

        chart.configure_mesh().draw()?;

        // Plot configured weights for each connection
        let colors = [&RED, &BLUE, &GREEN, &MAGENTA];
        for (i, connection) in self.connections.iter().enumerate() {
            let configured_data: Vec<(f64, f64)> = stats
                .iter()
                .map(|s| {
                    let weight = if i < s.configured_weights.len() {
                        s.configured_weights[i]
                    } else {
                        0.0
                    };
                    (s.timestamp.as_secs_f64(), weight)
                })
                .collect();

            chart
                .draw_series(LineSeries::new(configured_data, colors[i % colors.len()]))?
                .label(format!("{} (Configured)", connection.name))
                .legend(move |(x, y)| PathElement::new(vec![(x, y), (x + 10, y)], colors[i % colors.len()]));

            // Plot actual distributions (normalized)
            let actual_data: Vec<(f64, f64)> = stats
                .iter()
                .map(|s| {
                    let total: u64 = s.actual_distributions.iter().sum();
                    let actual_ratio = if total > 0 && i < s.actual_distributions.len() {
                        s.actual_distributions[i] as f64 / total as f64
                    } else {
                        0.0
                    };
                    (s.timestamp.as_secs_f64(), actual_ratio)
                })
                .collect();

            chart
                .draw_series(LineSeries::new(actual_data, colors[i % colors.len()].mix(0.5)))?
                .label(format!("{} (Actual)", connection.name))
                .legend(move |(x, y)| {
                    PathElement::new(vec![(x, y), (x + 10, y)], colors[i % colors.len()].mix(0.5))
                });
        }

        chart.configure_series_labels().draw()?;
        Ok(())
    }

    fn plot_network_metrics(
        &self,
        area: &DrawingArea<SVGBackend, plotters::coord::Shift>,
        stats: &[PerformanceStats],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut chart = ChartBuilder::on(area)
            .caption("Network Performance Metrics", ("sans-serif", 30))
            .margin(10)
            .x_label_area_size(40)
            .y_label_area_size(50)
            .build_cartesian_2d(
                0f64..stats.last().unwrap().timestamp.as_secs_f64(),
                0f64..100f64, // Percentage scale for loss
            )?;

        chart.configure_mesh().draw()?;

        // Plot packet loss for each connection
        let colors = [&RED, &BLUE, &GREEN, &MAGENTA];
        for (i, connection) in self.connections.iter().enumerate() {
            let loss_data: Vec<(f64, f64)> = stats
                .iter()
                .map(|s| {
                    let loss = if i < s.packet_loss.len() {
                        s.packet_loss[i]
                    } else {
                        0.0
                    };
                    (s.timestamp.as_secs_f64(), loss)
                })
                .collect();

            chart
                .draw_series(LineSeries::new(loss_data, colors[i % colors.len()]))?
                .label(format!("{} Loss %", connection.name))
                .legend(move |(x, y)| PathElement::new(vec![(x, y), (x + 10, y)], colors[i % colors.len()]));
        }

        chart.configure_series_labels().draw()?;
        Ok(())
    }
}

/// Create a 1080p60 H.265 video source with dual-tone AAC stereo audio, muxed to MPEG-TS then RTP stream
fn create_ts_av_source() -> Result<gst::Element, Box<dyn std::error::Error>> {
    let bin = gst::Bin::new();

    // Video pipeline: videotestsrc -> convert -> caps(1080p60) -> x265enc -> h265parse -> queue -> mpegtsmux
    let videotestsrc = gst::ElementFactory::make("videotestsrc")
        .property("is-live", true)
        .property_from_str("pattern", "smpte")
        .build()?;

    let videoconvert = gst::ElementFactory::make("videoconvert").build()?;
    let video_caps = gst::Caps::builder("video/x-raw")
        .field("format", "I420")
        .field("width", 1920i32)
        .field("height", 1080i32)
        .field("framerate", gst::Fraction::new(60, 1))
        .build();
    let videocaps = gst::ElementFactory::make("capsfilter")
        .property("caps", &video_caps)
        .build()?;
    let x265enc = gst::ElementFactory::make("x265enc")
        .property("bitrate", 6000u32)
        .property_from_str("tune", "zerolatency")
        .property_from_str("speed-preset", "superfast")
        // Disable NUMA thread pools in libx265 to avoid set_mempolicy warnings in containers
        .property_from_str("option-string", "pools=none")
        .build()?;
    let h265parse = gst::ElementFactory::make("h265parse")
        .property("config-interval", 1i32)
        .build()?;
    let vqueue = gst::ElementFactory::make("queue").build()?;

    // Audio pipeline: single stereo source -> convert -> caps -> aacenc -> aacparse -> queue -> mpegtsmux
    // Use single stereo source instead of interleaving two mono sources to avoid channel layout issues
    let audiotestsrc = gst::ElementFactory::make("audiotestsrc")
        .property("freq", 440.0f64)
        .property("is-live", true)
        .build()?;
    let audioconvert = gst::ElementFactory::make("audioconvert").build()?;
    let audioresample = gst::ElementFactory::make("audioresample").build()?;
    let audio_caps = gst::Caps::builder("audio/x-raw")
        .field("channels", 2i32)
        .field("rate", 48000i32)
        .field("layout", "interleaved")
        .field("channel-mask", 0x3u64) // FL | FR (front left, front right)
        .build();
    let audiocaps = gst::ElementFactory::make("capsfilter")
        .property("caps", &audio_caps)
        .build()?;
    let aacenc = gst::ElementFactory::make("avenc_aac")
        .property("bitrate", 128000i32)
        .build()?;
    let aacparse = gst::ElementFactory::make("aacparse").build()?;
    let aqueue = gst::ElementFactory::make("queue").build()?;

    // MPEG-TS muxer then payload to RTP using rtpmp2tpay
    let mpegtsmux = gst::ElementFactory::make("mpegtsmux").build()?;
    let mqueue = gst::ElementFactory::make("queue").build()?;
    let rtpmp2tpay = gst::ElementFactory::make("rtpmp2tpay")
        .property("pt", 33u32)
        .build()?;

    // Add to bin
    bin.add_many([
        &videotestsrc, &videoconvert, &videocaps, &x265enc, &h265parse, &vqueue,
        &audiotestsrc, &audioconvert, &audioresample, &audiocaps, &aacenc, &aacparse, &aqueue,
        &mpegtsmux, &mqueue, &rtpmp2tpay,
    ])?;

    // Link video chain
    videotestsrc.link(&videoconvert)?;
    videoconvert.link(&videocaps)?;
    videocaps.link(&x265enc)?;
    x265enc.link(&h265parse)?;
    h265parse.link(&vqueue)?;
    // Connect to mpegtsmux sink pad
    let vsrc = vqueue.static_pad("src").unwrap();
    let vpad = mpegtsmux
        .request_pad_simple("sink_%d")
        .ok_or("Failed to request video pad on mpegtsmux")?;
    vsrc.link(&vpad)?;

    // Link audio chain (single stereo source)
    gst::Element::link_many([&audiotestsrc, &audioconvert, &audioresample, &audiocaps, &aacenc, &aacparse, &aqueue])?;
    // Connect to mpegtsmux sink pad
    let asrc = aqueue.static_pad("src").unwrap();
    let apad = mpegtsmux
        .request_pad_simple("sink_%d")
        .ok_or("Failed to request audio pad on mpegtsmux")?;
    asrc.link(&apad)?;

    // Mux -> queue -> rtpmp2tpay
    mpegtsmux.link(&mqueue)?;
    mqueue.link(&rtpmp2tpay)?;

    // Ghost src pad
    let src_pad = rtpmp2tpay.static_pad("src").unwrap();
    let ghost = gst::GhostPad::with_target(&src_pad).unwrap();
    bin.add_pad(&ghost)?;

    Ok(bin.upcast())
}

/// Create a MPEG-TS recorder that receives single-stream TS-over-RTP and writes the MPEG-TS file
fn create_ts_recorder(output_file: &str) -> Result<gst::Element, Box<dyn std::error::Error>> {
    let bin = gst::Bin::new();
    // Pipeline: [RTP input] -> queue -> rtpjitterbuffer -> rtpmp2tdepay -> filesink (.ts)
    let queue = gst::ElementFactory::make("queue").build()?;
    let rtpjitterbuffer = gst::ElementFactory::make("rtpjitterbuffer")
        .property("latency", 200u32)
        .build()?;
    let rtpmp2tdepay = gst::ElementFactory::make("rtpmp2tdepay").build()?;
    let filesink = gst::ElementFactory::make("filesink")
        .property("location", output_file)
        .property("async", false)
        .build()?;

    bin.add_many([&queue, &rtpjitterbuffer, &rtpmp2tdepay, &filesink])?;

    queue.link(&rtpjitterbuffer)?;
    rtpjitterbuffer.link(&rtpmp2tdepay)?;
    rtpmp2tdepay.link(&filesink)?;

    // Ghost sink pad for RTP input
    let sink_pad = queue.static_pad("sink").unwrap();
    let ghost = gst::GhostPad::with_target(&sink_pad).unwrap();
    bin.add_pad(&ghost)?;

    Ok(bin.upcast())
}

#[test]
fn test_performance_evaluation_1080p60_four_bonded_connections() {
    init_for_tests();

    println!("=== Performance Evaluation: 1080p60 H.265 + AAC MPEG-TS over 4 Bonded Connections ===");

    // Define four different connection profiles
    let connections = vec![
        ConnectionProfile::connection_5g_good(),
        ConnectionProfile::connection_4g_good(),
        ConnectionProfile::connection_4g_typical(),
        ConnectionProfile::connection_5g_poor(),
    ];

    println!("Connection profiles:");
    for (i, conn) in connections.iter().enumerate() {
        println!("  {}: {} - {}-{}kbps (base: {}kbps), {}ms delay, {}% loss, varies every {:.1}s", 
                i, conn.name, conn.min_bitrate_kbps, conn.max_bitrate_kbps, 
                conn.base_bitrate_kbps, conn.base_delay_ms, conn.loss_percent, 
                conn.bandwidth_variation_period_s);
    }

    // Create performance collector
    let collector = PerformanceCollector::new(connections.clone());

    // Create pipeline elements
    let pipeline = gst::Pipeline::new();
    
    // Create MPEG-TS AV source (H.265 + AAC -> MPEG-TS -> RTP)
    let av_source = create_ts_av_source()
        .expect("Failed to create TS AV source");

    // Create dispatcher with initial equal weights for four connections
    let initial_weights = vec![0.25, 0.25, 0.25, 0.25];
    let dispatcher = create_dispatcher_for_testing(Some(&initial_weights));
    
    // Enable auto-balancing and metrics
    dispatcher.set_property("auto-balance", true);
    dispatcher.set_property("rebalance-interval-ms", 1000u64);
    dispatcher.set_property("metrics-export-interval-ms", 500u64);

    // Create dynamic bitrate controller
    let dynbitrate = gst::ElementFactory::make("dynbitrate")
        .build()
        .expect("Failed to create dynbitrate");

    // Create counter sinks for each bonded connection
    let mut counters = Vec::new();
    for i in 0..4 {
        let counter = create_counter_sink();
        counter.set_property("name", format!("counter_{}", i));
        counters.push(counter);
    }

    // Create MPEG-TS file recorder (RTP -> TS depayload -> file)
    let output_file = format!("/workspace/target/performance_test_{}fps_h265_aac.ts", 
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs());
    println!("Recording MPEG-TS to: {}", output_file);
    
    let file_recorder = create_ts_recorder(&output_file)
        .expect("Failed to create TS recorder");

    // Add all elements to pipeline
    pipeline.add(&av_source).expect("Failed to add AV source");
    pipeline.add(&dispatcher).expect("Failed to add dispatcher");
    pipeline.add(&dynbitrate).expect("Failed to add dynbitrate");
    pipeline.add(&file_recorder).expect("Failed to add file recorder");
    
    for counter in &counters {
        pipeline.add(counter).expect("Failed to add counter");
    }

    // Link the pipeline
    av_source.link(&dynbitrate).expect("Failed to link AV source to dynbitrate");
    dynbitrate.link(&dispatcher).expect("Failed to link dynbitrate to dispatcher");

    // Request src pads and link to counters
    let mut src_pads = Vec::new();
    for (i, counter) in counters.iter().enumerate() {
        let src_pad = dispatcher.request_pad_simple("src_%u")
            .unwrap_or_else(|| panic!("Failed to request src pad {}", i));
        src_pad.link(&counter.static_pad("sink").unwrap())
            .unwrap_or_else(|_| panic!("Failed to link src pad {} to counter", i));
        src_pads.push(src_pad);
    }
    
    // Link first connection to video recorder for file output (record one stream)
    if !src_pads.is_empty() {
        // Create a tee to split first connection's stream, with per-branch queues to prevent blocking
        let recorder_tee = gst::ElementFactory::make("tee")
            .build()
            .expect("Failed to create recorder tee");
        pipeline.add(&recorder_tee).expect("Failed to add recorder tee");
        let tee_q_counter = gst::ElementFactory::make("queue").build().expect("Failed to create tee queue for counter");
        let tee_q_rec = gst::ElementFactory::make("queue").build().expect("Failed to create tee queue for recorder");
        pipeline.add_many([&tee_q_counter, &tee_q_rec]).expect("Failed to add tee queues");
        
        // Insert tee between first src pad and counter
        let first_src_pad = &src_pads[0];
        let first_counter = &counters[0];
        
        // Unlink first connection from counter
        let _ = first_src_pad.unlink(&first_counter.static_pad("sink").unwrap());
        
        // Link src pad to tee, then tee to both counter and recorder
        first_src_pad.link(&recorder_tee.static_pad("sink").unwrap())
            .expect("Failed to link first src to tee");
            
        let tee_src1 = recorder_tee.request_pad_simple("src_%u").unwrap();
        let tee_src2 = recorder_tee.request_pad_simple("src_%u").unwrap();

        // Link tee -> queues -> sinks
        tee_src1.link(&tee_q_counter.static_pad("sink").unwrap())
            .expect("Failed to link tee to counter queue");
        tee_q_counter
            .link_pads(Some("src"), first_counter, Some("sink"))
            .expect("Failed to link counter queue to counter");

        tee_src2.link(&tee_q_rec.static_pad("sink").unwrap())
            .expect("Failed to link tee to recorder queue");
        tee_q_rec
            .link_pads(Some("src"), &file_recorder, Some("sink"))
            .expect("Failed to link recorder queue to file recorder");
    }
    println!("Starting performance test...");

    // Start the pipeline
    pipeline.set_state(gst::State::Playing)
        .expect("Failed to start pipeline");

    // Collect statistics over test duration (30 seconds)
    let test_duration = Duration::from_secs(30);
    let stats_interval = Duration::from_millis(500);
    let start_time = Instant::now();

    while start_time.elapsed() < test_duration {
        thread::sleep(stats_interval);
        
        if let Err(e) = collector.collect_stats(&dispatcher, &counters) {
            eprintln!("Failed to collect stats: {}", e);
        }

        // Print progress
        let elapsed = start_time.elapsed();
        let progress = elapsed.as_secs_f64() / test_duration.as_secs_f64() * 100.0;
        print!("\rProgress: {:.1}%", progress);
        std::io::Write::flush(&mut std::io::stdout()).unwrap();
    }

    println!("\nStopping pipeline...");

    // Send EOS event to properly close files and flush all buffered data
    println!("Sending EOS to properly flush all data to disk...");
    if !pipeline.send_event(gst::event::Eos::new()) {
        eprintln!("Warning: Failed to send EOS event");
    }
    
    // Wait for EOS to propagate through the entire pipeline
    // This ensures all elements (especially filesink) flush their buffers
    let bus = pipeline.bus().unwrap();
    let timeout = gst::ClockTime::from_seconds(5);
    match bus.timed_pop_filtered(timeout, &[gst::MessageType::Eos, gst::MessageType::Error]) {
        Some(msg) => match msg.view() {
            gst::MessageView::Eos(_) => {
                println!("EOS received - all data should be flushed to disk");
            }
            gst::MessageView::Error(err) => {
                eprintln!("Error during EOS: {} - {}", err.error(), err.debug().unwrap_or_default());
            }
            _ => {}
        },
        None => {
            eprintln!("Warning: EOS timeout - proceeding with shutdown anyway");
        }
    }

    // Step down states to complete shutdown
    let _ = pipeline.set_state(gst::State::Ready);
    let _ = pipeline.set_state(gst::State::Null);

    // Generate performance plots
    let svg_output = "/workspace/target/performance_analysis.svg";
    collector.generate_svg_plots(svg_output)
        .expect("Failed to generate performance plots");

    // Print final statistics
    let final_stats = collector.stats.lock().unwrap();
    println!("\n=== Final Performance Results ===");
    println!("Total samples collected: {}", final_stats.len());
    
    if let Some(last_stats) = final_stats.last() {
        println!("Final weight distribution:");
        for (i, weight) in last_stats.configured_weights.iter().enumerate() {
            let total_packets: u64 = last_stats.actual_distributions.iter().sum();
            let actual_ratio = if total_packets > 0 {
                last_stats.actual_distributions[i] as f64 / total_packets as f64
            } else {
                0.0
            };
            println!("  Connection {}: Configured={:.3}, Actual={:.3}, Packets={}", 
                    i, weight, actual_ratio, last_stats.actual_distributions[i]);
        }
    }

    println!("Performance evaluation completed!");
    println!("Output MPEG-TS stream saved to: {}", output_file);
    println!("Performance plots saved to: {}", svg_output);
}