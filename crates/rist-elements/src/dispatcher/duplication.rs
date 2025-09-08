use gstreamer as gst;
use gstreamer::prelude::PadExt;

use crate::dispatcher::state::DispatcherInner;

pub(crate) fn is_keyframe(buffer: &gst::Buffer) -> bool {
    !buffer.flags().contains(gst::BufferFlags::DELTA_UNIT)
}

pub(crate) fn can_duplicate_keyframe(
    inner: &DispatcherInner,
    state: &mut crate::dispatcher::state::State,
) -> bool {
    let now = std::time::Instant::now();
    let budget_pps = *inner.dup_budget_pps.lock();
    if let Some(reset_time) = state.dup_budget_reset_time {
        if now.duration_since(reset_time).as_secs() >= 1 {
            state.dup_budget_used = 0;
            state.dup_budget_reset_time = Some(now);
        }
    } else {
        state.dup_budget_reset_time = Some(now);
    }
    if state.dup_budget_used < budget_pps {
        state.dup_budget_used += 1;
        true
    } else {
        false
    }
}

pub(crate) fn duplicate_keyframe_to_backup(
    inner: &DispatcherInner,
    srcpads: &[gst::Pad],
    current_idx: usize,
    buffer: &gst::Buffer,
) {
    let (swrr_counters, health_timers, scheduler, quantum_bytes) = {
        let state = inner.state.lock();
        (
            state.swrr_counters.clone(),
            state.link_health_timers.clone(),
            *inner.scheduler.lock(),
            *inner.quantum_bytes.lock() as i64,
        )
    };
    let health_warmup_ms = *inner.health_warmup_ms.lock();

    let now = std::time::Instant::now();
    let mut best_backup_idx = None;
    let mut best_counter = f64::NEG_INFINITY;
    for (i, pad) in srcpads.iter().enumerate() {
        if i == current_idx || !pad.is_linked() {
            continue;
        }
        let is_healthy = if let Some(health_start) = health_timers.get(i) {
            let health_duration = now.duration_since(*health_start).as_millis() as u64;
            health_duration >= health_warmup_ms
        } else {
            true
        };
        if !is_healthy {
            continue;
        }
        if let Some(&counter) = swrr_counters.get(i) {
            if counter > best_counter {
                best_counter = counter;
                best_backup_idx = Some(i);
            }
        }
    }

    if let Some(backup_idx) = best_backup_idx {
        if let Some(backup_pad) = srcpads.get(backup_idx) {
            let res = backup_pad.push(buffer.clone());
            if res.is_ok() && scheduler == crate::dispatcher::state::Scheduler::Drr {
                let mut st = inner.state.lock();
                if backup_idx < st.drr_deficits.len() {
                    let new_def = st.drr_deficits[backup_idx] - buffer.size() as i64;
                    let floor = -4 * quantum_bytes;
                    st.drr_deficits[backup_idx] = new_def.max(floor);
                }
            }
        }
    }
}
