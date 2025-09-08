use crate::dispatcher::state::{DispatcherInner, State};

pub(crate) fn calculate_aimd_weights(inner: &DispatcherInner, state: &mut State) -> bool {
    let rtx_threshold = *inner.aimd_rtx_threshold.lock();
    let rtt_threshold = 200.0;
    let additive_increase = 0.1;
    let multiplicative_decrease = 0.5;
    let mut changed = false;

    let old_weights = state.weights.clone();

    for (i, stats) in state.link_stats.iter().enumerate() {
        if i >= state.weights.len() {
            break;
        }
        let current_weight = state.weights[i];
        if stats.ewma_rtx_rate < rtx_threshold && stats.ewma_rtt < rtt_threshold {
            state.weights[i] = (current_weight + additive_increase).min(2.0);
        } else {
            state.weights[i] = (current_weight * multiplicative_decrease).max(0.05);
        }
    }

    let total: f64 = state.weights.iter().sum();
    if total > 0.0 {
        for w in &mut state.weights {
            *w /= total;
        }
    }

    for (old, new) in old_weights.iter().zip(state.weights.iter()) {
        if (old - new).abs() > 0.01 {
            changed = true;
            break;
        }
    }

    if changed {
        let quantum = *inner.quantum_bytes.lock() as i64;
        let floor = -4 * quantum;
        for d in &mut state.drr_deficits {
            if *d < floor {
                *d = floor;
            }
        }
    }

    changed
}
