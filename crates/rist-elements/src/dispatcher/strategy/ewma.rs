use crate::dispatcher::state::{DispatcherInner, State};

pub(crate) fn calculate_ewma_weights(inner: &DispatcherInner, state: &mut State) -> bool {
    let count = state.weights.len();
    if count == 0 {
        return false;
    }

    let base_eps = *inner.probe_ratio.lock();
    let alpha_penalty = *inner.ewma_rtx_penalty.lock();
    let beta_penalty = *inner.ewma_rtt_penalty.lock();
    let elapsed = state.started_at.elapsed().as_secs_f64();

    let prev_weights = if state.weights.is_empty() {
        vec![1.0 / count as f64; count]
    } else {
        state.weights.clone()
    };

    let mut scores = vec![0.0; count];
    for i in 0..count {
        if let Some(stats) = state.link_stats.get(i) {
            let delivered_pps = if stats.prev_rr_received > 0 && stats.ewma_delivered_pps > 0.0 {
                stats.ewma_delivered_pps
            } else {
                stats.ewma_goodput
            }
            .max(1.0);

            let loss_term = (1.0 - stats.ewma_rtx_rate)
                .max(0.05)
                .powf(2.2 + alpha_penalty);
            let reliability_penalty = 1.0 / (1.0 + 20.0 * stats.ewma_rtx_rate.max(0.0).powf(1.2));
            let rtt_term = 1.0 / (1.0 + beta_penalty * (stats.ewma_rtt / 45.0).max(0.2));
            scores[i] = (delivered_pps * loss_term * rtt_term * reliability_penalty).max(1e-6);
        }
    }

    let mut score_sum: f64 = scores.iter().sum();
    if score_sum <= 0.0 {
        scores.fill(1.0);
        score_sum = scores.iter().sum();
    }
    let mut new_weights: Vec<f64> = scores.iter().map(|s| s / score_sum).collect();

    let base_smoothing = if elapsed < 5.0 {
        0.45
    } else if elapsed < 20.0 {
        0.65
    } else {
        0.85
    };
    for (i, w) in new_weights.iter_mut().enumerate() {
        let prev = prev_weights
            .get(i)
            .copied()
            .unwrap_or_else(|| 1.0 / count as f64);
        let raw_weight = *w;
        let delta = (raw_weight - prev).abs();
        let adapt = (delta / 0.2).clamp(0.0, 1.0);
        let smoothing = (base_smoothing - adapt * 0.3).clamp(0.35, 0.9);
        *w = smoothing * raw_weight + (1.0 - smoothing) * prev;
    }

    let mut sum = new_weights.iter().sum::<f64>();
    if sum <= 0.0 {
        return false;
    }
    for w in &mut new_weights {
        *w /= sum;
    }

    let cap = *inner.max_link_share.lock();
    if cap < 1.0 {
        let mut capped = vec![false; count];
        let mut remaining = 1.0;
        let mut iter = 0;
        loop {
            iter += 1;
            if iter > count + 1 {
                break;
            }
            let mut under_sum = 0.0;
            for (i, &w) in new_weights.iter().enumerate() {
                if !capped[i] {
                    under_sum += w;
                }
            }
            if under_sum <= 0.0 {
                let uncapped = capped.iter().filter(|&&c| !c).count();
                if uncapped > 0 {
                    let fill = remaining / uncapped as f64;
                    for (i, w) in new_weights.iter_mut().enumerate() {
                        if !capped[i] {
                            *w = fill.min(cap);
                        }
                    }
                }
                break;
            }
            let scale = remaining / under_sum;
            let mut any_new_cap = false;
            for (i, w) in new_weights.iter_mut().enumerate() {
                if capped[i] {
                    continue;
                }
                let proposed = *w * scale;
                if proposed > cap {
                    *w = cap;
                    capped[i] = true;
                    any_new_cap = true;
                } else {
                    *w = proposed;
                }
            }
            let new_remaining = 1.0 - new_weights.iter().sum::<f64>();
            if !any_new_cap || new_remaining.abs() < 1e-9 {
                break;
            } else {
                remaining = new_remaining.max(0.0);
            }
        }
    }

    let probe_boost = *inner.probe_boost.lock();
    let probe_period = *inner.probe_period_ms.lock();
    if probe_boost > 0.0 && !new_weights.is_empty() {
        let now = std::time::Instant::now();
        if now.duration_since(state.last_probe).as_millis() as u64 >= probe_period {
            state.probe_idx = (state.probe_idx + 1) % new_weights.len();
            state.last_probe = now;
        }
        let idx = state.probe_idx.min(new_weights.len() - 1);
        new_weights[idx] *= 1.0 + probe_boost;
        sum = new_weights.iter().sum::<f64>();
        if sum > 0.0 {
            for w in &mut new_weights {
                *w /= sum;
            }
        }
    }

    let elapsed = state.started_at.elapsed().as_secs_f64();
    let mut eps = base_eps;
    if elapsed < 5.0 {
        eps = eps.max(0.12);
    }
    if count > 0 && eps > 0.0 {
        let mix = eps / count as f64;
        for w in &mut new_weights {
            *w = (1.0 - eps) * *w + mix;
        }
    }

    let mut changed = false;
    for (old, new) in state.weights.iter().zip(new_weights.iter()) {
        if (old - new).abs() > 0.01 {
            changed = true;
            break;
        }
    }
    if changed || state.weights.len() != new_weights.len() {
        state.weights = new_weights;
        state.swrr_counters.fill(0.0);
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
