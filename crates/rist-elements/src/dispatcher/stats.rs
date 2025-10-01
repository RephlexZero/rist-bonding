use gst::glib;
use gstreamer as gst;
use gstreamer::prelude::GstObjectExt;
use gstreamer::prelude::{Cast, ObjectExt};

use crate::dispatcher::element::Dispatcher;
use crate::dispatcher::state::{DispatcherInner, LinkStats, State, Strategy};

pub(crate) fn poll_rist_stats_and_update_weights(inner: &DispatcherInner) {
    let rist_element = inner.rist_element.lock().clone();
    if let Some(rist) = rist_element {
        let stats_value: glib::Value = rist.property("stats");
        if let Ok(Some(structure)) = stats_value.get::<Option<gst::Structure>>() {
            update_weights_from_stats(inner, &structure);
        } else if let Ok(structure) = stats_value.get::<gst::Structure>() {
            update_weights_from_stats(inner, &structure);
        }
    }
}

pub(crate) fn update_weights_from_stats(inner: &DispatcherInner, stats: &gst::Structure) {
    let strategy = *inner.strategy.lock();
    let mut state = inner.state.lock();
    let now = std::time::Instant::now();

    if let Ok(sess_stats_value) = stats.get::<glib::Value>("session-stats") {
        if let Ok(sess_array) = sess_stats_value.get::<glib::ValueArray>() {
            let num_sessions = sess_array.len();
            while state.link_stats.len() < num_sessions {
                state.link_stats.push(LinkStats::default());
            }
            for (arr_idx, session_value) in sess_array.iter().enumerate() {
                if let Ok(session_struct) = session_value.get::<gst::Structure>() {
                    let idx = arr_idx;
                    if state.link_stats.len() <= idx {
                        state.link_stats.resize(idx + 1, LinkStats::default());
                    }
                    let sent_original = session_struct
                        .get::<u64>("sent-original-packets")
                        .unwrap_or(0);
                    let sent_retrans = session_struct
                        .get::<u64>("sent-retransmitted-packets")
                        .unwrap_or(0);
                    let rr_have = session_struct
                        .get::<bool>("rr-have-report")
                        .unwrap_or(false);
                    let rr_highest = session_struct
                        .get::<u64>("rr-extended-highest-seq")
                        .or_else(|_| {
                            session_struct
                                .get::<u32>("rr-extended-highest-seq")
                                .map(|v| v as u64)
                        })
                        .unwrap_or(0);
                    let rr_packets_lost = session_struct
                        .get::<i64>("rr-packets-lost")
                        .or_else(|_| {
                            session_struct
                                .get::<i32>("rr-packets-lost")
                                .map(|v| v as i64)
                        })
                        .or_else(|_| {
                            session_struct
                                .get::<u64>("rr-packets-lost")
                                .map(|v| v as i64)
                        })
                        .unwrap_or(0);
                    let rtt_ms = session_struct
                        .get::<u64>("round-trip-time")
                        .map(|ns| ns as f64 / 1_000_000.0)
                        .or_else(|_| session_struct.get::<f64>("round-trip-time"))
                        .unwrap_or(50.0);

                    if let Some(link_stats) = state.link_stats.get_mut(idx) {
                        let delta_time =
                            now.duration_since(link_stats.prev_timestamp).as_secs_f64();
                        if delta_time > 0.1 {
                            let delta_original =
                                sent_original.saturating_sub(link_stats.prev_sent_original);
                            let delta_retrans =
                                sent_retrans.saturating_sub(link_stats.prev_sent_retransmitted);
                            let goodput = delta_original as f64 / delta_time;
                            let rtx_rate = if delta_original > 0 {
                                delta_retrans as f64 / (delta_original + delta_retrans) as f64
                            } else {
                                0.0
                            };
                            link_stats.ewma_goodput = link_stats.alpha * goodput
                                + (1.0 - link_stats.alpha) * link_stats.ewma_goodput;
                            link_stats.ewma_rtx_rate = link_stats.alpha * rtx_rate
                                + (1.0 - link_stats.alpha) * link_stats.ewma_rtx_rate;
                            link_stats.ewma_rtt = link_stats.alpha * rtt_ms
                                + (1.0 - link_stats.alpha) * link_stats.ewma_rtt;
                            if rr_have {
                                if std::env::var("EWMA_DEBUG").is_ok() {
                                    eprintln!(
                                        "rr_state link{}: prev_hi={} hi={} prev_lost={} lost={}",
                                        idx,
                                        link_stats.prev_rb_highest_seq,
                                        rr_highest,
                                        link_stats.prev_rb_packets_lost,
                                        rr_packets_lost
                                    );
                                }
                                link_stats.prev_rr_fraction =
                                    session_struct.get::<f64>("rr-fraction-lost").unwrap_or(0.0);
                                if link_stats.prev_rb_highest_seq > 0
                                    && rr_highest >= link_stats.prev_rb_highest_seq
                                {
                                    let delta_expected =
                                        rr_highest - link_stats.prev_rb_highest_seq;
                                    let prev_lost = link_stats.prev_rb_packets_lost.max(0);
                                    let lost_now = rr_packets_lost.max(0);
                                    let delta_lost = if lost_now >= prev_lost {
                                        (lost_now - prev_lost) as u64
                                    } else {
                                        0
                                    };
                                    let delta_delivered = delta_expected.saturating_sub(delta_lost);
                                    if delta_delivered > 0 {
                                        let delivered_pps = delta_delivered as f64 / delta_time;
                                        if std::env::var("EWMA_DEBUG").is_ok() {
                                            eprintln!(
                                                "rr_debug link{}: delta_recv={} pps={:.2}",
                                                idx, delta_delivered, delivered_pps
                                            );
                                        }
                                        link_stats.ewma_delivered_pps = link_stats.alpha
                                            * delivered_pps
                                            + (1.0 - link_stats.alpha)
                                                * link_stats.ewma_delivered_pps;
                                        link_stats.prev_rr_received = link_stats
                                            .prev_rr_received
                                            .saturating_add(delta_delivered);
                                    }
                                }
                                link_stats.prev_rb_highest_seq = rr_highest;
                                link_stats.prev_rb_packets_lost = rr_packets_lost;
                            }
                            link_stats.prev_sent_original = sent_original;
                            link_stats.prev_sent_retransmitted = sent_retrans;
                            link_stats.prev_timestamp = now;
                        }
                    }
                }
            }
        } else {
            update_weights_from_stats_legacy(&mut state, stats, now);
        }
    } else {
        update_weights_from_stats_legacy(&mut state, stats, now);
    }

    let weights_changed = match strategy {
        Strategy::Ewma => {
            crate::dispatcher::strategy::ewma::calculate_ewma_weights(inner, &mut state)
        }
        Strategy::Aimd => {
            crate::dispatcher::strategy::aimd::calculate_aimd_weights(inner, &mut state)
        }
    };

    if weights_changed {
        let weights_json = serde_json::to_string(&state.weights).unwrap_or_default();
        drop(state);
        if let Some(sinkpad) = inner.sinkpad.lock().as_ref() {
            if let Some(parent) = sinkpad.parent() {
                if let Ok(dispatcher) = parent.downcast::<Dispatcher>() {
                    dispatcher.emit_by_name::<()>("weights-changed", &[&weights_json]);
                    dispatcher.notify("current-weights");
                }
            }
        }
    }
}

pub(crate) fn update_weights_from_stats_legacy(
    state: &mut State,
    stats: &gst::Structure,
    now: std::time::Instant,
) {
    let num_links = state.weights.len();
    while state.link_stats.len() < num_links {
        state.link_stats.push(LinkStats::default());
    }
    for (link_idx, link_stats) in state.link_stats.iter_mut().enumerate() {
        let session_key = format!("session-{}", link_idx);
        if let Ok(sent_original) = stats
            .get::<u64>(&format!("{}.sent-original-packets", session_key))
            .or_else(|_| stats.get::<u64>("sent-original-packets"))
        {
            let sent_retrans = stats
                .get::<u64>(&format!("{}.sent-retransmitted-packets", session_key))
                .or_else(|_| stats.get::<u64>("sent-retransmitted-packets"))
                .unwrap_or(0);
            let rtt_ms = stats
                .get::<u64>(&format!("{}.round-trip-time", session_key))
                .or_else(|_| stats.get::<u64>("round-trip-time"))
                .map(|ns| ns as f64 / 1_000_000.0)
                .or_else(|_| stats.get::<f64>(&format!("{}.round-trip-time", session_key)))
                .or_else(|_| stats.get::<f64>("round-trip-time"))
                .unwrap_or(50.0);
            let delta_time = now.duration_since(link_stats.prev_timestamp).as_secs_f64();
            if delta_time > 0.1 {
                let delta_original = sent_original.saturating_sub(link_stats.prev_sent_original);
                let delta_retrans = sent_retrans.saturating_sub(link_stats.prev_sent_retransmitted);
                let goodput = delta_original as f64 / delta_time;
                let rtx_rate = if delta_original > 0 {
                    delta_retrans as f64 / (delta_original + delta_retrans) as f64
                } else {
                    0.0
                };
                link_stats.ewma_goodput =
                    link_stats.alpha * goodput + (1.0 - link_stats.alpha) * link_stats.ewma_goodput;
                link_stats.ewma_rtx_rate = link_stats.alpha * rtx_rate
                    + (1.0 - link_stats.alpha) * link_stats.ewma_rtx_rate;
                link_stats.ewma_rtt =
                    link_stats.alpha * rtt_ms + (1.0 - link_stats.alpha) * link_stats.ewma_rtt;
                link_stats.prev_sent_original = sent_original;
                link_stats.prev_sent_retransmitted = sent_retrans;
                link_stats.prev_timestamp = now;
                link_stats.prev_rb_highest_seq = 0;
                link_stats.prev_rb_packets_lost = 0;
                link_stats.prev_rr_received = 0;
            }
        }
    }
}
