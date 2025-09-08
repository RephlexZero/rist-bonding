use gstreamer as gst;
use gstreamer::prelude::{Cast, ElementExt, GstBinExt, GstObjectExt, ObjectExt};

use crate::dispatcher::element::Dispatcher;
use crate::dispatcher::state::DispatcherInner;

pub(crate) fn emit_metrics_message(inner: &DispatcherInner) {
    let state = inner.state.lock();
    let selected_index = state.next_out;
    let weights = state.weights.clone();
    drop(state);

    let encoder_bitrate = if let Some(sinkpad) = inner.sinkpad.lock().as_ref() {
        if let Some(parent) = sinkpad.parent() {
            if let Some(pipeline) = parent.parent() {
                if let Ok(bin) = pipeline.downcast::<gst::Bin>() {
                    if let Some(dynbitrate) = bin.by_name("dynbitrate") {
                        dynbitrate.property::<u32>("bitrate")
                    } else {
                        0
                    }
                } else {
                    0
                }
            } else {
                0
            }
        } else {
            0
        }
    } else {
        0
    };

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let buffers_processed = 0u64;
    let src_pad_count = weights.len() as u32;

    let current_weights_json = serde_json::to_string(&weights).unwrap_or_default();
    let ewma_rtx_penalty = *inner.ewma_rtx_penalty.lock();
    let ewma_rtt_penalty = *inner.ewma_rtt_penalty.lock();
    let aimd_rtx_threshold = *inner.aimd_rtx_threshold.lock();

    if let Some(sinkpad) = inner.sinkpad.lock().as_ref() {
        if let Some(parent) = sinkpad.parent() {
            if let Ok(dispatcher) = parent.downcast::<Dispatcher>() {
                let structure = gst::Structure::builder("rist-dispatcher-metrics")
                    .field("timestamp", timestamp)
                    .field("current-weights", current_weights_json.as_str())
                    .field("buffers-processed", buffers_processed)
                    .field("src-pad-count", src_pad_count)
                    .field("selected-index", selected_index as u32)
                    .field("encoder-bitrate", encoder_bitrate)
                    .field("ewma-rtx-penalty", ewma_rtx_penalty)
                    .field("ewma-rtt-penalty", ewma_rtt_penalty)
                    .field("aimd-rtx-threshold", aimd_rtx_threshold)
                    .build();
                let message = gst::message::Application::builder(structure)
                    .src(&dispatcher)
                    .build();
                let _ = dispatcher.post_message(message);
            }
        }
    }
}
