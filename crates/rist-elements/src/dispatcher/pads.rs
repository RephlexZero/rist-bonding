use gst::prelude::*;
use gst::subclass::prelude::ElementImpl;
use gstreamer as gst;
use std::sync::Arc;

use crate::dispatcher::state::DispatcherInner;

pub(crate) fn build_sink_pad(inner: &Arc<DispatcherInner>) -> gst::Pad {
    let sink_template = super::element::DispatcherImpl::pad_templates()
        .iter()
        .find(|tmpl| tmpl.name() == "sink")
        .unwrap();

    let inner_weak = Arc::downgrade(inner);
    gst::Pad::builder_from_template(sink_template)
        .name("sink")
        .chain_function(move |_pad, _parent, buf| {
            let inner = match inner_weak.upgrade() {
                Some(inner) => inner,
                None => {
                    return Err(gst::FlowError::Flushing);
                }
            };
            super::element::DispatcherImpl::handle_chain(&inner, buf)
        })
        .event_function({
            let inner_weak = Arc::downgrade(inner);
            move |pad, parent, event| {
                if let Some(inner) = inner_weak.upgrade() {
                    super::element::DispatcherImpl::handle_sink_event(&inner, pad, parent, event)
                } else {
                    false
                }
            }
        })
        .query_function({
            let inner_weak = Arc::downgrade(inner);
            move |pad, parent, query| {
                if let Some(inner) = inner_weak.upgrade() {
                    super::element::DispatcherImpl::handle_sink_query(&inner, pad, parent, query)
                } else {
                    false
                }
            }
        })
        .build()
}
