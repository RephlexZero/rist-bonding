use gstreamer as gst;
use once_cell::sync::Lazy;

pub(crate) static CAT: Lazy<gst::DebugCategory> = Lazy::new(|| {
    gst::DebugCategory::new(
        "ristdispatcher",
        gst::DebugColorFlags::empty(),
        Some("RIST Dispatcher"),
    )
});

#[allow(clippy::too_many_arguments)]
pub(crate) fn pick_output_index_swrr_with_hysteresis(
    weights: &[f64],
    swrr_counters: &mut Vec<f64>,
    current_idx: usize,
    last_switch_time: Option<std::time::Instant>,
    min_hold_ms: u64,
    _switch_threshold: f64,
    health_warmup_ms: u64,
    link_health_timers: &[std::time::Instant],
) -> (usize, bool) {
    if weights.is_empty() {
        gst::warning!(CAT, "Empty weights array, using index 0");
        return (0, false);
    }

    let n = weights.len();
    if swrr_counters.len() != n {
        gst::debug!(
            CAT,
            "SWRR counters length mismatch, resizing from {} to {}",
            swrr_counters.len(),
            n
        );
        swrr_counters.resize(n, 0.0);
    }

    let now = std::time::Instant::now();

    let in_hold_period = if let Some(last_switch) = last_switch_time {
        let since_switch = now.duration_since(last_switch).as_millis() as u64;
        since_switch < min_hold_ms
    } else {
        false
    };

    let mut adjusted_weights = weights.to_vec();
    for (i, &health_start) in link_health_timers.iter().enumerate() {
        if i < adjusted_weights.len() {
            let health_duration = now.duration_since(health_start).as_millis() as u64;
            if health_duration < health_warmup_ms {
                let health_factor = health_duration as f64 / health_warmup_ms as f64;
                let penalty = 0.5 * (1.0 - health_factor);
                adjusted_weights[i] *= 1.0 - penalty;
            }
        }
    }

    for (counter, &weight) in swrr_counters.iter_mut().zip(adjusted_weights.iter()) {
        *counter += weight;
    }

    let mut best_idx = 0;
    let mut best_value = swrr_counters[0];

    for (i, &value) in swrr_counters.iter().enumerate() {
        if value > best_value {
            best_value = value;
            best_idx = i;
        }
    }

    if in_hold_period && current_idx < n {
        let weight_sum: f64 = adjusted_weights.iter().sum();
        if weight_sum > 0.0 {
            swrr_counters[current_idx] -= weight_sum;
        }
        return (current_idx, false);
    }

    let selected_idx = best_idx;
    let weight_sum: f64 = adjusted_weights.iter().sum();
    if weight_sum > 0.0 {
        swrr_counters[selected_idx] -= weight_sum;
    }

    (selected_idx, selected_idx != current_idx)
}

// Removed unused pick_output_index_swrr testing helper
