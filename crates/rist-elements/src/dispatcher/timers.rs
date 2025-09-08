use crate::dispatcher::state::DispatcherInner;
use gst::glib;
use gstreamer as gst;
use std::time::Duration;

#[allow(dead_code)]
pub(crate) fn start_rebalancer_timer(inner: &DispatcherInner) {
    let auto_balance = *inner.auto_balance.lock();
    if !auto_balance {
        return;
    }
    let inner_weak = std::sync::Arc::downgrade(&std::sync::Arc::new(inner));
    // Note: we can't upgrade Arc from a borrowed ref; callers should own an Arc. This shim is
    // only for structure; actual start is performed from element with proper Arc.
    drop(inner_weak);
}

pub(crate) fn start_metrics_timer(inner: &std::sync::Arc<DispatcherInner>) {
    let interval_ms = *inner.metrics_export_interval_ms.lock();
    if interval_ms == 0 {
        return;
    }
    if let Some(existing_id) = inner.metrics_timeout_id.lock().take() {
        existing_id.remove();
    }
    let inner_weak = std::sync::Arc::downgrade(inner);
    let timeout_id = gst::glib::timeout_add(Duration::from_millis(interval_ms), move || {
        if let Some(inner) = inner_weak.upgrade() {
            crate::dispatcher::metrics::emit_metrics_message(&inner);
            glib::ControlFlow::Continue
        } else {
            glib::ControlFlow::Break
        }
    });
    *inner.metrics_timeout_id.lock() = Some(timeout_id);
}

pub(crate) fn stop_metrics_timer(inner: &DispatcherInner) {
    if let Some(existing_id) = inner.metrics_timeout_id.lock().take() {
        existing_id.remove();
    }
}
