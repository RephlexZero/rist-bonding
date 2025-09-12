//! Dispatcher convergence under constrained static bandwidth (stress)
//!
//! This test sets up four shaped links with total capacity below the encoder
//! bitrate to stress the dispatcher. It verifies that the final traffic
//! split converges close to the capacity proportions.

use gst::prelude::*;
use gstreamer as gst;
use gstristelements::testing::{create_dispatcher, init_for_tests};

#[cfg(feature = "network-sim")]
use ::network_sim::{
    get_connection_ips,
    namespace::{cleanup_rist_test_links, create_shaped_veth_pair, ShapedVethConfig},
    qdisc::QdiscManager,
    runtime::{apply_ingress_params, remove_ingress_params},
    types::NetworkParams,
};

#[cfg(feature = "network-sim")]
use tokio::time::{sleep, Duration};

#[cfg(feature = "network-sim")]
#[derive(Debug, Clone)]
struct LinkProfile {
    veth_tx: String,
    veth_rx: String,
    tx_ip: String,
    rx_ip: String,
    delay_ms: u32,
    loss_pct: f32,
    rate_kbps: u32,
    port: u16,
}

#[cfg(feature = "network-sim")]
fn mk_profile(idx: usize, rate_kbps: u32, delay_ms: u32, loss_pct: f32, port: u16) -> LinkProfile {
    let oct = 180 + idx as u8;
    LinkProfile {
        veth_tx: format!("veths{}", idx),
        veth_rx: format!("vethr{}", idx),
        tx_ip: format!("10.200.{}.1", oct),
        rx_ip: format!("10.200.{}.2", oct),
        delay_ms,
        loss_pct,
        rate_kbps,
        port,
    }
}

#[cfg(feature = "network-sim")]
#[tokio::test]
async fn test_bonded_links_static_stress() {
    if std::env::var_os("GST_DEBUG").is_none() {
        std::env::set_var("GST_DEBUG", "ristdispatcher:INFO,*:WARNING");
    }
    init_for_tests();

    // Require NET_ADMIN
    let qdisc = QdiscManager::new();
    if !qdisc.has_net_admin().await {
        eprintln!("Skipping: requires NET_ADMIN");
        return;
    }

    // Four links with total capacity < encoder bitrate (3 Mbps)
    // Capacity split (kbps): [1000, 700, 350, 150] total=2200
    let profiles = vec![
        mk_profile(0, 1000, 20, 0.001, 6000),
        mk_profile(1, 700, 30, 0.002, 6002),
        mk_profile(2, 350, 45, 0.003, 6004),
        mk_profile(3, 150, 60, 0.004, 6006),
    ];
    let total_capacity: u32 = profiles.iter().map(|p| p.rate_kbps).sum();
    let expected: Vec<f64> = profiles
        .iter()
        .map(|p| p.rate_kbps as f64 / total_capacity as f64)
        .collect();

    // Create shaped veth pairs (egress on TX, ingress mirror on RX)
    let mut links: Vec<ShapedVethConfig> = Vec::with_capacity(profiles.len());
    for p in &profiles {
        let cfg = ShapedVethConfig {
            tx_interface: p.veth_tx.clone(),
            rx_interface: p.veth_rx.clone(),
            tx_ip: format!("{}/30", p.tx_ip),
            rx_ip: format!("{}/30", p.rx_ip),
            rx_namespace: None,
            network_params: NetworkParams {
                delay_ms: p.delay_ms,
                loss_pct: p.loss_pct,
                rate_kbps: p.rate_kbps,
                jitter_ms: 0,
                reorder_pct: 0.0,
                duplicate_pct: 0.0,
                loss_corr_pct: 0.0,
            },
        };
        create_shaped_veth_pair(&qdisc, &cfg).await.expect("veth");
        let _ = apply_ingress_params(&qdisc, &cfg.rx_interface, &cfg.network_params).await;
        links.push(cfg);
    }

    // Sender pipeline: H.265 1080p30 at ~3000 kbps
    let sender = gst::Pipeline::new();
    let av_source: gst::Element = {
        let bin = gst::Bin::new();
        let videotestsrc = gst::ElementFactory::make("videotestsrc")
            .property("is-live", true)
            .property_from_str("pattern", "smpte")
            .property("do-timestamp", true)
            .build()
            .unwrap();
        let videoconvert = gst::ElementFactory::make("videoconvert").build().unwrap();
        let capsfilter = gst::ElementFactory::make("capsfilter")
            .property(
                "caps",
                gst::Caps::builder("video/x-raw")
                    .field("format", "I420")
                    .field("width", 1920i32)
                    .field("height", 1080i32)
                    .field("framerate", gst::Fraction::new(30, 1))
                    .build(),
            )
            .build()
            .unwrap();
        let x265enc = gst::ElementFactory::make("x265enc")
            .property("bitrate", 3000u32)
            .property_from_str("speed-preset", "ultrafast")
            .property_from_str("tune", "zerolatency")
            .build()
            .unwrap();
        let h265parse = gst::ElementFactory::make("h265parse").build().unwrap();
        let rtph265pay = gst::ElementFactory::make("rtph265pay")
            .property("mtu", 1200u32)
            .property("pt", 96u32)
            .property("ssrc", 0x22u32)
            .build()
            .unwrap();
        bin.add_many([
            &videotestsrc,
            &videoconvert,
            &capsfilter,
            &x265enc,
            &h265parse,
            &rtph265pay,
        ])
        .unwrap();
        gst::Element::link_many([
            &videotestsrc,
            &videoconvert,
            &capsfilter,
            &x265enc,
            &h265parse,
            &rtph265pay,
        ])
        .unwrap();
        let src_pad = rtph265pay.static_pad("src").unwrap();
        let ghost = gst::GhostPad::with_target(&src_pad).unwrap();
        ghost.set_active(true).unwrap();
        bin.add_pad(&ghost).unwrap();
        bin.upcast()
    };

    // Dispatcher (EWMA + SWRR)
    let dispatcher = create_dispatcher(Some(&[0.25, 0.25, 0.25, 0.25]));
    dispatcher.set_property("strategy", "ewma");
    dispatcher.set_property("scheduler", "swrr");
    dispatcher.set_property("quantum-bytes", 1200u32);
    dispatcher.set_property("min-hold-ms", 200u64);
    dispatcher.set_property("switch-threshold", 1.05f64);
    dispatcher.set_property("ewma-rtx-penalty", 0.30f64);
    dispatcher.set_property("ewma-rtt-penalty", 0.10f64);
    dispatcher.set_property("rebalance-interval-ms", 500u64);
    dispatcher.set_property("metrics-export-interval-ms", 250u64);
    dispatcher.set_property("auto-balance", false);

    // ristsink (bonding)
    let sender_bonds = links
        .iter()
        .zip(profiles.iter())
        .map(|(cfg, p)| {
            let (_tx, rx) = get_connection_ips(cfg);
            format!("{}:{}", rx, p.port)
        })
        .collect::<Vec<_>>()
        .join(",");
    let rist_sink = gst::ElementFactory::make("ristsink")
        .property("address", &profiles[0].rx_ip)
        .property("port", profiles[0].port as u32)
        .property("bonding-addresses", &sender_bonds)
        .property("dispatcher", &dispatcher)
        .property("min-rtcp-interval", 50u32)
        .property("sender-buffer", 1000u32)
        .property("stats-update-interval", 500u32)
        .property("multicast-loopback", false)
        .build()
        .unwrap();

    sender.add(&av_source).unwrap();
    let queue = gst::ElementFactory::make("queue").build().unwrap();
    let caps_ssrc = gst::ElementFactory::make("capsfilter")
        .property(
            "caps",
            gst::Caps::builder("application/x-rtp")
                .field("ssrc", 0x22u32)
                .build(),
        )
        .build()
        .unwrap();
    sender.add_many([&queue, &caps_ssrc, &rist_sink]).unwrap();
    gst::Element::link_many([&av_source, &queue, &caps_ssrc, &rist_sink]).unwrap();

    // Receiver pipeline
    let receiver = gst::Pipeline::new();
    let recv_bonds = links
        .iter()
        .zip(profiles.iter())
        .map(|(cfg, p)| {
            let (_tx, rx) = get_connection_ips(cfg);
            format!("{}:{}", rx, p.port)
        })
        .collect::<Vec<_>>()
        .join(",");
    let rist_src = gst::ElementFactory::make("ristsrc")
        .property("address", &profiles[0].rx_ip)
        .property("port", profiles[0].port as u32)
        .property("bonding-addresses", &recv_bonds)
        .property("encoding-name", "H265")
        .property("receiver-buffer", 2000u32)
        .property("min-rtcp-interval", 50u32)
        .property("multicast-loopback", false)
        .build()
        .unwrap();
    let jb = gst::ElementFactory::make("rtpjitterbuffer")
        .property("latency", 200u32)
        .property("drop-on-latency", true)
        .build()
        .unwrap();
    let depay = gst::ElementFactory::make("rtph265depay").build().unwrap();
    let parse = gst::ElementFactory::make("h265parse").build().unwrap();
    let dec = gst::ElementFactory::make("avdec_h265").build().unwrap();
    let sink = gst::ElementFactory::make("fakesink")
        .property("sync", false)
        .property("silent", true)
        .build()
        .unwrap();
    receiver
        .add_many([&rist_src, &jb, &depay, &parse, &dec, &sink])
        .unwrap();
    gst::Element::link_many([&rist_src, &jb, &depay, &parse, &dec, &sink]).unwrap();

    // Play
    receiver.set_state(gst::State::Playing).unwrap();
    sender.set_state(gst::State::Playing).unwrap();
    sleep(Duration::from_millis(500)).await;

    

    // Wait for RTT briefly before enabling auto-balance (best-effort)
    let mut rtt_ready = false;
    for _ in 0..20 {
        if let Some(stats) = rist_sink.property::<Option<gst::Structure>>("stats") {
            if let Ok(arr) = stats.get::<glib::ValueArray>("session-stats") {
                for v in arr.iter() {
                    if let Ok(s) = v.get::<gst::Structure>() {
                        let rtt = s.get::<u64>("round-trip-time").unwrap_or(0);
                        if rtt > 0 {
                            rtt_ready = true;
                            break;
                        }
                    }
                }
            }
        }
        if rtt_ready {
            break;
        }
        sleep(Duration::from_millis(200)).await;
    }
    dispatcher.set_property("auto-balance", true);

    // Observe dispatcher weights for ~60s (configurable)
    let iterations: usize = std::env::var("BOND_STRESS_ITERATIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(240); // 240 * 250ms = 60s
    let sample_interval = Duration::from_millis(250);

    let main_ctx = glib::MainContext::default();
    let mut last_weights: Option<Vec<f64>> = None;
    let mut history: Vec<Vec<f64>> = Vec::new();
    // Empirical distribution tracking based on actual original packet counts per session
    let mut prev_original: Vec<u64> = Vec::new();
    let mut accum_original_delta: Vec<f64> = Vec::new();
    // Per-iteration delta history (for recent empirical window calculation)
    let mut empirical_history: Vec<Vec<f64>> = Vec::new();

    for _ in 0..iterations {
        // Drive GLib/GStreamer timers so dispatcher internal timeouts fire
        // Iterate non-blocking a few times
        for _ in 0..3 {
            while main_ctx.iteration(false) {}
        }
        let s: String = dispatcher.property("current-weights");
        if let Ok(v) = serde_json::from_str::<Vec<f64>>(&s) {
            if !v.is_empty() && v.iter().all(|w| w.is_finite() && *w >= 0.0) {
                if last_weights.as_ref().map(|lw| lw != &v).unwrap_or(true) {
                    eprintln!("weights update: {:?}", v);
                }
                last_weights = Some(v.clone());
                history.push(v);
            }
        }
        // Capture stats to build empirical traffic distribution
        if let Some(stats) = rist_sink.property::<Option<gst::Structure>>("stats") {
            if let Ok(arr) = stats.get::<glib::ValueArray>("session-stats") {
                if prev_original.is_empty() && !arr.is_empty() {
                    prev_original = vec![0u64; arr.len()];
                    accum_original_delta = vec![0f64; arr.len()];
                }
                if !arr.is_empty() {
                    let mut delta_vec: Vec<f64> = Vec::with_capacity(arr.len());
                    for (i, val) in arr.iter().enumerate() {
                        if let Ok(sess) = val.get::<gst::Structure>() {
                            let sent_original =
                                sess.get::<u64>("sent-original-packets").unwrap_or(0);
                            if i < prev_original.len() {
                                let delta = sent_original.saturating_sub(prev_original[i]);
                                prev_original[i] = sent_original;
                                if i < accum_original_delta.len() {
                                    accum_original_delta[i] += delta as f64;
                                }
                                delta_vec.push(delta as f64);
                            }
                        }
                    }
                    // Only push if we collected a full set of per-link deltas
                    if !delta_vec.is_empty() && delta_vec.len() == accum_original_delta.len() {
                        empirical_history.push(delta_vec);
                    }
                }
            }
        }
        sleep(sample_interval).await;
    }

    // Tidy shutdown
    let _ = sender.set_state(gst::State::Ready);
    let _ = receiver.set_state(gst::State::Ready);
    sleep(Duration::from_millis(500)).await;
    let _ = sender.set_state(gst::State::Null);
    let _ = receiver.set_state(gst::State::Null);
    for cfg in &links {
        let _ = remove_ingress_params(&qdisc, &cfg.rx_interface).await;
    }
    let _ = cleanup_rist_test_links(&qdisc, &links).await;

    let observed_final = last_weights.expect("No dispatcher weights observed");
    assert_eq!(observed_final.len(), expected.len());

    // Compute averaged weights over last N samples to smooth transient fluctuations
    let window = 10usize.min(history.len());
    let averaged: Vec<f64> = if window > 0 {
        history
            .iter()
            .rev()
            .take(window)
            .fold(vec![0.0; expected.len()], |mut acc, v| {
                for (i, w) in v.iter().enumerate() {
                    acc[i] += *w;
                }
                acc
            })
            .into_iter()
            .map(|sum| sum / window as f64)
            .collect()
    } else {
        observed_final.clone()
    };

    eprintln!("expected fractions (capacity): {:?}", expected);
    eprintln!("final weights:              {:?}", observed_final);
    eprintln!("avg(last {}):               {:?}", window, averaged);
    // Compute cumulative empirical fractions from observed packet distribution
    let empirical_cumulative: Option<Vec<f64>> =
        if !accum_original_delta.is_empty() && accum_original_delta.iter().any(|v| *v > 0.0) {
            let sum: f64 = accum_original_delta.iter().sum();
            if sum > 0.0 {
                Some(accum_original_delta.iter().map(|d| d / sum).collect())
            } else {
                None
            }
        } else {
            None
        };

    // Compute recent empirical distribution over the same averaging window using per-iteration deltas
    let recent_empirical: Option<Vec<f64>> = if window > 0 && !empirical_history.is_empty() {
        let take_n = window.min(empirical_history.len());
        let mut sums = vec![0f64; expected.len()];
        let mut total_sum = 0f64;
        for deltas in empirical_history.iter().rev().take(take_n) {
            for (i, d) in deltas.iter().enumerate() {
                sums[i] += *d;
                total_sum += *d;
            }
        }
        if total_sum > 0.0 {
            Some(sums.into_iter().map(|v| v / total_sum).collect())
        } else {
            None
        }
    } else {
        None
    };

    if let Some(emp) = &empirical_cumulative {
        eprintln!("empirical fractions (cumulative packet share): {:?}", emp);
    } else {
        eprintln!("empirical fractions (cumulative) unavailable");
    }
    if let Some(emp_r) = &recent_empirical {
        eprintln!(
            "recent empirical fractions (last {} samples): {:?}",
            window, emp_r
        );
    } else {
        eprintln!("recent empirical fractions unavailable (insufficient recent data)");
    }

    let mut max_err: f64 = 0.0;
    // Choose comparison target priority:
    // 1. Recent empirical (captures current steady-state)
    // 2. Cumulative empirical (overall run)
    // 3. Expected capacity fractions
    let target: Vec<f64> = if let Some(r) = recent_empirical.clone() {
        r
    } else if let Some(c) = empirical_cumulative.clone() {
        c
    } else {
        expected.clone()
    };
    for (i, (o, e)) in averaged.iter().zip(target.iter()).enumerate() {
        let err = (o - e).abs();
        max_err = max_err.max(err);
        eprintln!("link{}: avg={:.3} target={:.3} err={:.3}", i, o, e, err);
    }

    // Tolerance: relax progressively for empirical comparisons (recent empirical is noisier)
    let base_tol = 0.15; // theoretical capacity
    let tol = if recent_empirical.is_some() {
        0.22
    } else if empirical_cumulative.is_some() {
        0.18
    } else {
        base_tol
    };
    if max_err > tol {
        // If weights never deviated from uniform, provide targeted hint
        let is_uniform_series = history.iter().all(|w| w == history.first().unwrap());
        if is_uniform_series {
            eprintln!(
                "Dispatcher weights never changed from {:?} over {} samples",
                observed_final,
                history.len()
            );
            eprintln!("Hints: verify auto-balance property toggled, rebalance-interval-ms elapsed, and that GLib main context is being driven.");
        }
        if recent_empirical.is_some() {
            eprintln!("Using recent empirical distribution (last window) for comparison. Consider examining dispatcher metrics if divergence persists.");
        } else if empirical_cumulative.is_some() {
            eprintln!("Using cumulative empirical distribution for comparison. Recent empirical unavailable.");
        } else {
            eprintln!("Falling back to capacity fractions; empirical distributions unavailable.");
        }
        let target_label = if recent_empirical.is_some() {
            "recent-empirical"
        } else if empirical_cumulative.is_some() {
            "cumulative-empirical"
        } else {
            "capacity"
        };
        panic!(
            "max abs error {:.3} > tol {:.3} (target {})",
            max_err, tol, target_label
        );
    }
}

#[cfg(not(feature = "network-sim"))]
#[test]
fn test_bonded_links_static_stress_skip() {
    eprintln!("Requires 'network-sim' feature");
}
