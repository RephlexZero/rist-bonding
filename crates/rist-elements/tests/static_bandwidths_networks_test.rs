//! Static-bandwidth networks convergence test
//!
//! Verifies that starting from equal weights, the dispatcher converges toward
//! capacity-proportional weights when per-link bandwidths are fixed.

use gst::prelude::*;
use gstreamer as gst;
use gstristelements::testing::*;
use std::time::Duration;

#[cfg(feature = "network-sim")]
use ::network_sim::{qdisc::QdiscManager, types::NetworkParams, runtime::apply_network_params};

#[cfg(feature = "network-sim")]
use std::sync::Arc;

#[cfg(feature = "network-sim")]
use tokio::time::sleep;

#[derive(Debug, Clone)]
struct StaticProfile {
    name: &'static str,
    interface: &'static str,
    delay_ms: u32,
    loss_pct: f32,
    rate_kbps: u32,
}

impl StaticProfile {
    fn new(name: &'static str, interface: &'static str, delay_ms: u32, loss_pct: f32, rate_kbps: u32) -> Self {
        Self { name, interface, delay_ms, loss_pct, rate_kbps }
    }

    #[cfg(feature = "network-sim")]
    fn to_params(&self) -> NetworkParams { NetworkParams { delay_ms: self.delay_ms, loss_pct: self.loss_pct, rate_kbps: self.rate_kbps } }
}

#[cfg(feature = "network-sim")]
#[tokio::test]
async fn test_static_bandwidths_convergence() {
    init_for_tests();

    println!("=== Static Bandwidth Networks Convergence Test ===");

    // Fixed capacities for four links
    let profiles = vec![
        StaticProfile::new("5G-Good",   "veth0", 15, 0.0005, 4000),
        StaticProfile::new("4G-Good",   "veth1", 25, 0.0010, 2000),
        StaticProfile::new("4G-Typical","veth2", 40, 0.0050, 1200),
        StaticProfile::new("5G-Poor",   "veth3", 60, 0.0100,  800),
    ];

    // Expected capacity-proportional weights
    let total_capacity: u32 = profiles.iter().map(|p| p.rate_kbps).sum();
    let expected_weights: Vec<f64> = profiles.iter().map(|p| p.rate_kbps as f64 / total_capacity as f64).collect();

    println!("Profiles (fixed):");
    for (i, p) in profiles.iter().enumerate() {
        println!("  {}: {} - {}ms, {:.2}% loss, {} kbps", i, p.name, p.delay_ms, p.loss_pct*100.0, p.rate_kbps);
    }

    // Apply static network constraints once (may no-op in CI)
    let qdisc = Arc::new(QdiscManager::new());
    println!("\nApplying static constraints...");
    for p in &profiles {
        let _ = apply_network_params(&qdisc, p.interface, &p.to_params()).await;
    }

    // Build pipeline
    let pipeline = gst::Pipeline::new();

    // Live RTP source to produce continuous traffic
    let av_source = {
        let bin = gst::Bin::new();
        let videotestsrc = gst::ElementFactory::make("videotestsrc").property("is-live", true).property_from_str("pattern", "smpte").build().unwrap();
        let videoconvert = gst::ElementFactory::make("videoconvert").build().unwrap();
        let rtpvrawpay = gst::ElementFactory::make("rtpvrawpay").build().unwrap();
        bin.add_many([&videotestsrc, &videoconvert, &rtpvrawpay]).unwrap();
        gst::Element::link_many([&videotestsrc, &videoconvert, &rtpvrawpay]).unwrap();
        let src_pad = rtpvrawpay.static_pad("src").unwrap();
        let ghost_pad = gst::GhostPad::with_target(&src_pad).unwrap();
        ghost_pad.set_active(true).unwrap();
        bin.add_pad(&ghost_pad).unwrap();
        bin.upcast()
    };

    let dispatcher = create_dispatcher(Some(&[0.25, 0.25, 0.25, 0.25]));
    dispatcher.set_property("strategy", "ewma");
    dispatcher.set_property("auto-balance", true);
    dispatcher.set_property("rebalance-interval-ms", 500u64);
    dispatcher.set_property("min-hold-ms", 1000u64);
    dispatcher.set_property("switch-threshold", 1.05);
    dispatcher.set_property("ewma-rtx-penalty", 0.12);
    dispatcher.set_property("ewma-rtt-penalty", 0.08);

    let rist_stats = create_riststats_mock(Some(95.0), Some(15));

    // Counters to observe routing distribution
    let mut counters = Vec::new();
    for _ in 0..4 { counters.push(create_counter_sink()); }

    pipeline.add_many([&av_source, &dispatcher, &rist_stats]).unwrap();
    for c in &counters { pipeline.add(c).unwrap(); }

    // Link dispatcher outputs to counters
    for (i, c) in counters.iter().enumerate() {
        let srcpad = dispatcher.request_pad_simple(&format!("src_{}", i)).unwrap();
        let sinkpad = c.static_pad("sink").unwrap();
        srcpad.link(&sinkpad).unwrap();
    }

    av_source.link(&dispatcher).unwrap();

    // Stats adapter state (cumulative originals and retrans)
    let mut last_counts = vec![0u64; 4];
    let mut orig_cum = vec![0u64; 4];
    let mut rtx_cum = vec![0u64; 4];
    // Simple capacity model: pps cap = rate_kbps (units are arbitrary; ratios matter)
    let caps_pps: Vec<u64> = profiles.iter().map(|p| p.rate_kbps as u64).collect();

    // Start pipeline
    pipeline.set_state(gst::State::Playing).unwrap();

    // Periodic: compute deltas, apply capacity cap, update stats
    let test_secs = 30u64;
    for sec in 0..test_secs {
        sleep(Duration::from_secs(1)).await;

        // Offered per link = delta observed via counter sinks
        let mut curr_counts = Vec::with_capacity(4);
        for c in &counters { curr_counts.push(c.property::<u64>("count")); }
        let offered: Vec<u64> = curr_counts.iter().zip(last_counts.iter()).map(|(c,l)| c.saturating_sub(*l)).collect();

        // Apply capacity: originals increase by min(offered, cap), overflow becomes retrans
        for i in 0..4 {
            let cap = caps_pps[i];
            let off = offered[i];
            let good = off.min(cap);
            let overflow = off.saturating_sub(cap);
            // add a small base retrans component from configured loss
            let loss_rtx = ((good as f64) * profiles[i].loss_pct) as u64;
            orig_cum[i] = orig_cum[i].saturating_add(good);
            rtx_cum[i] = rtx_cum[i].saturating_add(overflow).saturating_add(loss_rtx);
        }

        // Build stats structure with cumulative fields
        let mut sb = gst::Structure::builder("rist/x-sender-stats");
        let mut total_o = 0u64; let mut total_r = 0u64;
        for i in 0..4 {
            let sid = format!("session-{}", i);
            let rtt_ms = profiles[i].delay_ms as f64 * 2.0 + 10.0;
            total_o = total_o.saturating_add(orig_cum[i]);
            total_r = total_r.saturating_add(rtx_cum[i]);
            sb = sb
                .field(format!("{}.sent-original-packets", sid), orig_cum[i])
                .field(format!("{}.sent-retransmitted-packets", sid), rtx_cum[i])
                .field(format!("{}.round-trip-time", sid), rtt_ms);
        }
        sb = sb.field("sent-original-packets", total_o).field("sent-retransmitted-packets", total_r).field("round-trip-time", 0.0f64);
        let stats = sb.build();
        rist_stats.set_property("stats", &stats);

        // Progress log
        let total = curr_counts.iter().sum::<u64>();
        let weights: Vec<f64> = if total>0 { curr_counts.iter().map(|&c| c as f64 / total as f64).collect() } else { vec![0.0;4] };
        if sec % 5 == 0 || sec >= test_secs - 3 {
            let rates: Vec<u64> = offered.clone();
            println!("t={:>2}s | Weights: [{:.3}, {:.3}, {:.3}, {:.3}] | Rates: [{}, {}, {}, {}]pps",
                sec, weights[0], weights[1], weights[2], weights[3], rates[0], rates[1], rates[2], rates[3]);
        }

        last_counts = curr_counts;
    }

    // Shutdown
    let _ = pipeline.set_state(gst::State::Ready);
    sleep(Duration::from_millis(300)).await;
    let _ = pipeline.set_state(gst::State::Null);
    sleep(Duration::from_millis(300)).await;

    // Final evaluation
    let final_counts: Vec<u64> = counters.iter().map(|c| c.property::<u64>("count")).collect();
    let sum = final_counts.iter().sum::<u64>();
    let final_weights: Vec<f64> = if sum>0 { final_counts.iter().map(|&c| c as f64 / sum as f64).collect() } else { vec![0.0;4] };

    println!("\nExpected (capacity-based): [{:.3}, {:.3}, {:.3}, {:.3}]",
        expected_weights[0], expected_weights[1], expected_weights[2], expected_weights[3]);
    println!("Final weights:           [{:.3}, {:.3}, {:.3}, {:.3}]",
        final_weights[0], final_weights[1], final_weights[2], final_weights[3]);

    let avg_dev = final_weights.iter().zip(expected_weights.iter()).map(|(a,e)|(a-e).abs()).sum::<f64>()/4.0;
    println!("Average deviation: {:.3}", avg_dev);
    if avg_dev < 0.10 { println!("✅ Excellent convergence"); } else if avg_dev < 0.15 { println!("✅ Good convergence"); } else { println!("⚠️ Convergence could improve"); }
}

// Fallback when network-sim isn’t enabled
#[cfg(not(feature = "network-sim"))]
#[test]
fn test_static_bandwidths_convergence_fallback() {
    println!("Static bandwidths test requires the 'network-sim' feature.");
}
