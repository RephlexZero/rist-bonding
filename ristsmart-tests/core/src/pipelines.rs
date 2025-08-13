use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tracing::{info, warn};

use crate::emulation::LinkPorts;
use crate::metrics::{RistStatsSnapshot, SamplesContext};

#[derive(Default)]
pub struct SampleProbes {
    // Receiver cumulative payload bytes after rtph265depay
    bytes_total: Arc<Mutex<u64>>,
    last_bytes_total: u64,

    // Per-run store
    pub ctx: SamplesContext,

    // Handles to query stats
    rx_bus: Option<gst::Bus>,
    tx_bus: Option<gst::Bus>,
    tx_ristsink: Option<gst::Element>,
    tx_dispatcher: Option<gst::Element>,
    tx_dynbitrate: Option<gst::Element>,
}

impl SampleProbes {
    pub fn bytes_delta_since_last(&mut self) -> u64 {
        let current = *self.bytes_total.lock().unwrap();
        let delta = current.saturating_sub(self.last_bytes_total);
        self.last_bytes_total = current;
        delta
    }

    pub fn poll_stats(&self) -> Result<RistStatsSnapshot> {
        // For now, use fallback values since GStreamer property access needs proper typing
        let dyn_bitrate_kbps = self.ctx.last_dyn_bitrate_kbps;
        let dispatcher_weights = self.ctx.last_dispatcher_weights.clone();
        let rist_sessions = self.ctx.last_rist_sessions.clone();

        Ok(RistStatsSnapshot { 
            dyn_bitrate_kbps, 
            dispatcher_weights, 
            rist_sessions 
        })
    }

    pub fn record_sample(
        &mut self,
        start: Instant,
        achieved_bps: f64,
        theoretical_bps: f64,
        link_bytes: &[u64],
        emu_state: &crate::emulation::EmuState,
        stats: &RistStatsSnapshot,
        ideal_weights: &[f64],
    ) -> Result<()> {
        let ts_ms = start.elapsed().as_millis() as u64;

        // Cache last-seen stats for fallback
        self.ctx.last_dyn_bitrate_kbps = stats.dyn_bitrate_kbps;
        self.ctx.last_dispatcher_weights = stats.dispatcher_weights.clone();
        self.ctx.last_rist_sessions = stats.rist_sessions.clone();

        let sample = crate::metrics::Sample {
            ts_ms,
            achieved_bps,
            theoretical_bps,
            rx_bytes_total: *self.bytes_total.lock().unwrap(),
            link_bytes: link_bytes.to_vec(),
            capacities_mbps: emu_state.capacities_mbps.clone(),
            loss_rate: emu_state.loss_rate.clone(),
            delay_ms: emu_state.delay_ms.clone(),
            dyn_bitrate_kbps: stats.dyn_bitrate_kbps,
            dispatcher_weights: stats.dispatcher_weights.clone(),
            ideal_weights: Some(ideal_weights.to_vec()),
            sessions: stats.rist_sessions.clone(),
        };
        self.ctx.metrics.record(sample);
        Ok(())
    }

    pub fn into_context(
        self,
        run_id: String,
        outdir: std::path::PathBuf,
        links: usize,
        scenario: crate::scenarios::ScenarioKind,
        efficiency: f64,
    ) -> Result<crate::metrics::RunContext> {
        self.ctx.into_context(run_id, outdir, links, scenario, efficiency)
    }
}

pub fn set_state(p: &gst::Pipeline, s: gst::State) -> Result<()> {
    p.set_state(s).map_err(|e| anyhow::anyhow!("Failed to set pipeline state: {}", e))?;
    Ok(())
}

pub fn build_receiver_rist(
    links: usize, 
    link_ports: &[LinkPorts], 
    probes: &mut SampleProbes
) -> Result<gst::Pipeline> {
    let pipeline = gst::Pipeline::new();

    let ristsrc = gst::ElementFactory::make("ristsrc")
        .build()
        .context("ristsrc missing (install gst-plugins-bad with RIST)")?;
    
    // Configure RIST bonding addresses for multi-link setup
    if links == 1 && !link_ports.is_empty() {
        // Single link setup
        let port = link_ports[0].port;
        // Ensure port is even (RIST requirement)
        let rist_port = if port & 1 == 0 { port } else { port - 1 };
        
        ristsrc.set_property("address", "0.0.0.0");
        ristsrc.set_property("port", rist_port as u32);
        info!("Configured ristsrc for single link on port {} (adjusted from {})", rist_port, port);
    } else if links > 1 && link_ports.len() >= links {
        // Multi-link bonding setup
        let bonding_addresses: Vec<String> = (0..links)
            .map(|i| {
                let port = link_ports[i].port;
                // Ensure port is even (RIST requirement)
                let rist_port = if port & 1 == 0 { port } else { port - 1 };
                format!("0.0.0.0:{}", rist_port)
            })
            .collect();
        let bonding_str = bonding_addresses.join(",");
        
        // Set the first address/port as primary
        let first_port = link_ports[0].port;
        let first_rist_port = if first_port & 1 == 0 { first_port } else { first_port - 1 };
        
        ristsrc.set_property("address", "0.0.0.0");
        ristsrc.set_property("port", first_rist_port as u32);
        
        // Configure bonding addresses for all links
        ristsrc.set_property("bonding-addresses", &bonding_str);
        info!("Configured ristsrc for {} links with bonding addresses: {}", links, bonding_str);
    } else {
        warn!("Invalid link configuration: {} links but {} link_ports", links, link_ports.len());
    }

    let depay = gst::ElementFactory::make("rtph265depay")
        .build()
        .context("rtph265depay not available")?;
    let parse = gst::ElementFactory::make("h265parse")
        .build()
        .context("h265parse not available")?;
    let appsink = gst::ElementFactory::make("appsink")
        .name("sink")
        .build()
        .context("appsink not available")?;

    pipeline.add_many(&[&ristsrc, &depay, &parse, &appsink])?;
    gst::Element::link_many(&[&ristsrc, &depay, &parse, &appsink])?;

    // Probe bytes after depay for payload-level throughput
    let depay_src = depay.static_pad("src")
        .context("depay src pad not found")?;
    let bytes_total = probes.bytes_total.clone();
    
    depay_src.add_probe(gst::PadProbeType::BUFFER, move |_, info| {
        if let Some(gst::PadProbeData::Buffer(ref buf)) = info.data {
            let size = buf.size();
            let mut total = bytes_total.lock().unwrap();
            *total += size as u64;
        }
        gst::PadProbeReturn::Ok
    });

    probes.rx_bus = pipeline.bus();
    Ok(pipeline)
}

pub fn build_sender_rist(
    links: usize, 
    link_ports: &[LinkPorts], 
    encoder_bitrate_kbps: u32, 
    probes: &mut SampleProbes
) -> Result<gst::Pipeline> {
    let pipeline = gst::Pipeline::new();

    // Source and encoder: 1080p60 H.265
    let src = gst::ElementFactory::make("videotestsrc")
        .property("is-live", true)
        .build()
        .context("videotestsrc not available")?;
    
    let caps = gst::Caps::builder("video/x-raw")
        .field("format", "I420")
        .field("width", 1920i32)
        .field("height", 1080i32)
        .field("framerate", gst::Fraction::new(60, 1))
        .build();
    
    let capsf = gst::ElementFactory::make("capsfilter")
        .property("caps", &caps)
        .build()
        .context("capsfilter not available")?;
    
    let conv = gst::ElementFactory::make("videoconvert")
        .build()
        .context("videoconvert not available")?;
    
    let enc = gst::ElementFactory::make("x265enc")
        .property_from_str("tune", "zerolatency")
        .property_from_str("speed-preset", "ultrafast")
        .property("key-int-max", 60i32)
        .property("bitrate", encoder_bitrate_kbps) // kbps - u32, not i32
        .build()
        .context("x265enc missing (install gst-plugins-ugly)")?;
    
    let parse = gst::ElementFactory::make("h265parse")
        .build()
        .context("h265parse not available")?;
    
    let pay = gst::ElementFactory::make("rtph265pay")
        .build()
        .context("rtph265pay not available")?;

    let ristsink = gst::ElementFactory::make("ristsink")
        .build()
        .context("ristsink missing (install gst-plugins-bad with RIST)")?;

    // Configure RIST bonding addresses for multi-link setup
    if links == 1 && !link_ports.is_empty() {
        // Single link setup
        let port = link_ports[0].port;
        let ns_ip = link_ports[0].ns_ip;
        // Ensure port is even (RIST requirement)
        let rist_port = if port & 1 == 0 { port } else { port - 1 };
        
        ristsink.set_property("address", &ns_ip.to_string());
        ristsink.set_property("port", rist_port as u32);
        info!("Configured ristsink for single link to {}:{} (adjusted from {})", ns_ip, rist_port, port);
    } else if links > 1 && link_ports.len() >= links {
        // Multi-link bonding setup
        let bonding_addresses: Vec<String> = (0..links)
            .map(|i| {
                let port = link_ports[i].port;
                let ns_ip = link_ports[i].ns_ip;
                // Ensure port is even (RIST requirement)
                let rist_port = if port & 1 == 0 { port } else { port - 1 };
                format!("{}:{}", ns_ip, rist_port)
            })
            .collect();
        let bonding_str = bonding_addresses.join(",");
        
        // Set the first address/port as primary
        let first_port = link_ports[0].port;
        let first_ns_ip = link_ports[0].ns_ip;
        let first_rist_port = if first_port & 1 == 0 { first_port } else { first_port - 1 };
        
        ristsink.set_property("address", &first_ns_ip.to_string());
        ristsink.set_property("port", first_rist_port as u32);
        
        // Configure bonding addresses for all links
        ristsink.set_property("bonding-addresses", &bonding_str);
        
        // Set bonding method to broadcast for testing (using enum value)
        // From the source: GST_RIST_BONDING_METHOD_BROADCAST = 0, GST_RIST_BONDING_METHOD_ROUND_ROBIN = 1
        ristsink.set_property_from_str("bonding-method", "broadcast");
        
        info!("Configured ristsink for {} links with bonding addresses: {}", links, bonding_str);
    } else {
        warn!("Invalid link configuration: {} links but {} link_ports", links, link_ports.len());
    }

    // Try to inject custom dispatcher for intelligent bonding
    if let Ok(dispatcher) = gst::ElementFactory::make("ristdispatcher").build() {
        ristsink.set_property("dispatcher", &dispatcher);
        probes.tx_dispatcher = Some(dispatcher);
        info!("Installed custom RIST dispatcher");
    } else {
        info!("Using built-in RIST bonding (ristdispatcher not available)");
    }
    
    if let Ok(dynbit) = gst::ElementFactory::make("dynbitrate").build() {
        // Wire dynbitrate to control enc.bitrate
        // TODO: This will need to be implemented based on your actual API
        probes.tx_dynbitrate = Some(dynbit);
        info!("Installed dynamic bitrate controller");
    } else {
        info!("Using static bitrate (dynbitrate not available)");
    }

    // Assemble and link
    pipeline.add_many(&[&src, &capsf, &conv, &enc, &parse, &pay, &ristsink])?;
    gst::Element::link_many(&[&src, &capsf, &conv, &enc, &parse, &pay, &ristsink])?;

    probes.tx_bus = pipeline.bus();
    probes.tx_ristsink = Some(ristsink.clone());

    Ok(pipeline)
}

pub fn build_receiver_mock(probes: &mut SampleProbes) -> Result<gst::Pipeline> {
    // Control-plane-only; simple appsrc->queue->appsink with probes
    let pipeline = gst::Pipeline::new();
    let appsrc = gst::ElementFactory::make("appsrc")
        .build()
        .context("appsrc not available")?;
    let queue = gst::ElementFactory::make("queue")
        .build()
        .context("queue not available")?;
    let appsink = gst::ElementFactory::make("appsink")
        .name("sink")
        .build()
        .context("appsink not available")?;
    
    pipeline.add_many(&[&appsrc, &queue, &appsink])?;
    gst::Element::link_many(&[&appsrc, &queue, &appsink])?;
    
    // Set up mock data flow
    appsrc.set_property("is-live", true);
    appsrc.set_property("format", gst::Format::Time);
    
    let bytes_total = probes.bytes_total.clone();
    let sink_pad = appsink.static_pad("sink")
        .context("appsink sink pad not found")?;
    
    sink_pad.add_probe(gst::PadProbeType::BUFFER, move |_, info| {
        if let Some(gst::PadProbeData::Buffer(ref buf)) = info.data {
            let size = buf.size();
            let mut total = bytes_total.lock().unwrap();
            *total += size as u64;
        }
        gst::PadProbeReturn::Ok
    });

    Ok(pipeline)
}

pub fn build_sender_mock(
    encoder_bitrate_kbps: u32, 
    _probes: &mut SampleProbes
) -> Result<gst::Pipeline> {
    let pipeline = gst::Pipeline::new();
    
    // Implement a synthetic flow for control-plane testing
    let appsrc = gst::ElementFactory::make("appsrc")
        .property("is-live", true)
        .property("format", gst::Format::Time)
        .build()
        .context("appsrc not available")?;
    
    let queue = gst::ElementFactory::make("queue")
        .build()
        .context("queue not available")?;
    
    let fakesink = gst::ElementFactory::make("fakesink")
        .build()
        .context("fakesink not available")?;
    
    pipeline.add_many(&[&appsrc, &queue, &fakesink])?;
    gst::Element::link_many(&[&appsrc, &queue, &fakesink])?;
    
    // TODO: Drive synthetic data at target bitrate
    info!("Mock sender pipeline created with target bitrate {} kbps", encoder_bitrate_kbps);
    
    Ok(pipeline)
}
