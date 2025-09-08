use crate::dispatcher::state::{DispatcherInner, State};

pub(crate) fn calculate_ewma_weights(inner: &DispatcherInner, state: &mut State) -> bool {
    let mut new_weights = vec![0.0; state.weights.len()];
    let mut total = 0.0;

    let n = state.weights.len() as f64;
    let base_eps = *inner.probe_ratio.lock();
    let share_floor = if n > 0.0 {
        (base_eps.max(1e-9)) / n
    } else {
        0.0
    };

    for (i, stats) in state.link_stats.iter().enumerate() {
        if i >= new_weights.len() {
            break;
        }
        let last_share = state
            .weights
            .get(i)
            .copied()
            .unwrap_or_else(|| if n > 0.0 { 1.0 / n } else { 1.0 })
            .max(share_floor);
        let delivered = if stats.ewma_delivered_pps > 0.0 {
            stats.ewma_delivered_pps
        } else {
            stats.ewma_goodput
        };
        let cap_est = delivered / last_share;
        let gp = cap_est.max(1.0).powf(0.5);
        let alpha = *inner.ewma_rtx_penalty.lock();
        let beta = *inner.ewma_rtt_penalty.lock();
        let q_rtx = 1.0 / (1.0 + alpha * stats.ewma_rtx_rate);
        let q_rtt = 1.0 / (1.0 + beta * (stats.ewma_rtt / 50.0).max(0.1));
        let mut w = gp * q_rtx * q_rtt;
        w = w.max(1e-6);
        new_weights[i] = w;
        total += w;
    }

    if total <= 0.0 {
        return false;
    }
    for w in &mut new_weights {
        *w /= total;
    }

    let cap = *inner.max_link_share.lock();
    if cap < 1.0 {
        let mut capped = vec![false; new_weights.len()];
        let mut remaining = 1.0;
        let mut iter = 0;
        loop {
            iter += 1;
            if iter > new_weights.len() + 1 {
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
        let sum: f64 = new_weights.iter().sum();
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
    if n > 0.0 && eps > 0.0 {
        for w in &mut new_weights {
            *w = (1.0 - eps) * *w + eps / n;
        }
    }

    let mut changed = false;
    for (old, new) in state.weights.iter().zip(new_weights.iter()) {
        if (old - new).abs() > 0.01 {
            changed = true;
            break;
        }
    }
    if changed {
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
