use gstreamer as gst;
use gstreamer::prelude::PadExt;

use crate::dispatcher::state::LinkStats;

pub(crate) struct DrrPickParams<'a> {
    pub pkt_bytes: usize,
    pub weights: &'a [f64],
    pub deficits: &'a mut [i64],
    pub quantum_bytes: usize,
    pub link_stats: &'a [LinkStats],
    pub burst_state: &'a mut (usize, usize),
    pub min_burst_pkts: u32,
    pub srcpads: &'a [gst::Pad],
}

// Removed legacy pick_output_index_drr (unused)

pub(crate) fn pick_output_index_drr_burst_aware(p: DrrPickParams<'_>) -> usize {
    let n = p.weights.len().max(1);
    let pkt = p.pkt_bytes as i64;
    let quantum_f = p.quantum_bytes as f64;
    let (ref mut current_burst, ref mut last_selected) = p.burst_state;

    if *current_burst > 0 && (*current_burst as u32) < p.min_burst_pkts {
        if let Some(pad) = p.srcpads.get(*last_selected) {
            if pad.is_linked()
                && *last_selected < p.deficits.len()
                && p.deficits[*last_selected] >= pkt
            {
                return *last_selected;
            }
        }
    }

    let min_rtt = if p.link_stats.is_empty() {
        50.0
    } else {
        p.link_stats
            .iter()
            .map(|s| s.ewma_rtt)
            .fold(f64::INFINITY, f64::min)
            .max(1.0)
    };

    let max_rounds = 3.min(n);
    for _round in 0..=max_rounds {
        for off in 0..n {
            let i = off % n;
            if let Some(pad) = p.srcpads.get(i) {
                if pad.is_linked() && p.deficits[i] >= pkt {
                    if i != *last_selected {
                        *current_burst = 1;
                        *last_selected = i;
                    } else {
                        *current_burst += 1;
                    }
                    return i;
                }
            }
        }
        for (i, deficit) in p.deficits.iter_mut().enumerate() {
            if i < p.weights.len() && i < p.link_stats.len() {
                let rtt_ratio = (p.link_stats[i].ewma_rtt / min_rtt).max(1.0);
                let scaled_quantum = quantum_f * p.weights[i] * rtt_ratio.powf(0.8);
                *deficit += scaled_quantum as i64;
            } else if i < p.weights.len() {
                *deficit += (p.weights[i] * quantum_f) as i64;
            }
        }
    }

    let mut best = 0usize;
    let mut best_score = i64::MIN;
    for (i, &deficit) in p.deficits.iter().enumerate() {
        if let Some(pad) = p.srcpads.get(i) {
            if pad.is_linked() {
                let q = (p.weights.get(i).cloned().unwrap_or(0.0) * quantum_f) as i64;
                let score = deficit + q;
                if score > best_score {
                    best_score = score;
                    best = i;
                }
            }
        }
    }
    if best != *last_selected {
        *current_burst = 1;
        *last_selected = best;
    } else {
        *current_burst += 1;
    }
    best
}
