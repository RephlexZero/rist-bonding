use gst::glib;
use gst::prelude::*;
use gst::subclass::prelude::*;
use gstreamer as gst;
use once_cell::sync::Lazy;
use std::sync::Arc;
use std::time::Duration;

use super::scheduler::{drr::*, swrr::*};
use super::state::*;

static CAT: Lazy<gst::DebugCategory> = Lazy::new(|| {
    gst::DebugCategory::new(
        "ristdispatcher",
        gst::DebugColorFlags::empty(),
        Some("RIST Dispatcher"),
    )
});

glib::wrapper! {
    pub struct Dispatcher(ObjectSubclass<DispatcherImpl>) @extends gst::Element, gst::Object;
}

#[derive(Default)]
pub struct DispatcherImpl {
    pub(super) inner: Arc<DispatcherInner>,
}

#[glib::object_subclass]
impl ObjectSubclass for DispatcherImpl {
    const NAME: &'static str = "ristdispatcher";
    type Type = Dispatcher;
    type ParentType = gst::Element;
}

impl ObjectImpl for DispatcherImpl {
    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj();

        let sinkpad = crate::dispatcher::pads::build_sink_pad(&self.inner);
        obj.add_pad(&sinkpad).unwrap();
        *self.inner.sinkpad.lock() = Some(sinkpad);

        self.discover_rist_sink_parent();

        let obj = self.obj();
        let obj_weak = obj.downgrade();
        obj.upcast_ref::<gst::Object>()
            .connect_notify(Some("parent"), move |o, _| {
                if let Some(obj) = obj_weak.upgrade() {
                    let imp = obj.imp();
                    if o.parent().is_some() {
                        imp.discover_rist_sink_parent();
                    } else {
                        *imp.inner.rist_element.lock() = None;
                    }
                }
            });

        self.start_rebalancer_timer();
        self.start_flow_watchdog();
    }

    fn properties() -> &'static [glib::ParamSpec] {
        super::props::properties()
    }

    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: Lazy<Vec<glib::subclass::Signal>> = Lazy::new(|| {
            vec![glib::subclass::Signal::builder("weights-changed")
                .param_types([String::static_type()])
                .build()]
        });
        SIGNALS.as_ref()
    }

    fn set_property(&self, id: usize, value: &glib::Value, _pspec: &glib::ParamSpec) {
        match id {
            1 => {
                if let Ok(Some(s)) = value.get::<Option<String>>() {
                    if let Ok(weights) = serde_json::from_str::<Vec<f64>>(&s) {
                        let valid_weights: Vec<f64> = weights
                            .into_iter()
                            .map(|w| if w.is_finite() && w >= 0.0 { w } else { 1.0 })
                            .collect();
                        if !valid_weights.is_empty() {
                            {
                                let mut st = self.inner.state.lock();
                                st.weights = valid_weights.clone();
                                st.swrr_counters.fill(0.0);
                                st.drr_deficits.fill(0);
                                st.drr_ptr = 0;
                            }
                            let weights_json =
                                serde_json::to_string(&valid_weights).unwrap_or_default();
                            self.obj()
                                .emit_by_name::<()>("weights-changed", &[&weights_json]);
                            self.obj().notify("current-weights");
                        }
                    }
                }
            }
            2 => {
                let interval = value.get::<u64>().unwrap_or(500).clamp(100, 10000);
                *self.inner.rebalance_interval_ms.lock() = interval;
                self.start_rebalancer_timer();
            }
            3 => {
                let s = value
                    .get::<Option<String>>()
                    .unwrap_or(Some("ewma".to_string()));
                let strategy = if let Some(s) = s {
                    if s.eq_ignore_ascii_case("aimd") {
                        Strategy::Aimd
                    } else {
                        Strategy::Ewma
                    }
                } else {
                    Strategy::Ewma
                };
                *self.inner.strategy.lock() = strategy;
            }
            4 => {
                let caps_any = value.get::<bool>().unwrap_or(false);
                *self.inner.caps_any.lock() = caps_any;
            }
            5 => {
                let auto_balance = value.get::<bool>().unwrap_or(true);
                *self.inner.auto_balance.lock() = auto_balance;
                if auto_balance {
                    self.start_rebalancer_timer();
                } else {
                    self.stop_rebalancer_timer();
                }
            }
            6 => {
                let rist = value.get::<Option<gst::Element>>().ok().flatten();
                *self.inner.rist_element.lock() = rist.clone();
                if rist.is_some() {
                    self.start_stats_polling();
                }
            }
            7 => {}
            8 => {
                let v = value.get::<u64>().unwrap_or(500).min(10000);
                *self.inner.min_hold_ms.lock() = v;
            }
            9 => {
                let v = value.get::<f64>().unwrap_or(1.2).clamp(1.0, 10.0);
                *self.inner.switch_threshold.lock() = v;
            }
            10 => {
                let v = value.get::<u64>().unwrap_or(2000).min(30000);
                *self.inner.health_warmup_ms.lock() = v;
            }
            11 => {
                let v = value.get::<bool>().unwrap_or(false);
                *self.inner.duplicate_keyframes.lock() = v;
            }
            12 => {
                let v = value.get::<u32>().unwrap_or(5).min(100);
                *self.inner.dup_budget_pps.lock() = v;
            }
            13 => {
                let v = value.get::<u64>().unwrap_or(0).min(60000);
                *self.inner.metrics_export_interval_ms.lock() = v;
                if v > 0 {
                    crate::dispatcher::timers::start_metrics_timer(&self.inner);
                } else {
                    crate::dispatcher::timers::stop_metrics_timer(&self.inner);
                }
            }
            14 => {
                let v = value.get::<f64>().unwrap_or(0.1).clamp(0.0, 10.0);
                *self.inner.ewma_rtx_penalty.lock() = v;
            }
            15 => {
                let v = value.get::<f64>().unwrap_or(0.05).clamp(0.0, 10.0);
                *self.inner.ewma_rtt_penalty.lock() = v;
            }
            16 => {
                let v = value.get::<f64>().unwrap_or(0.05).clamp(0.0, 1.0);
                *self.inner.aimd_rtx_threshold.lock() = v;
            }
            17 => {
                let v = value.get::<f64>().unwrap_or(0.08).clamp(0.0, 0.5);
                *self.inner.probe_ratio.lock() = v;
            }
            18 => {
                let v = value.get::<f64>().unwrap_or(0.70).clamp(0.5, 1.0);
                *self.inner.max_link_share.lock() = v;
            }
            19 => {
                let v = value.get::<f64>().unwrap_or(0.12).clamp(0.0, 1.0);
                *self.inner.probe_boost.lock() = v;
            }
            20 => {
                let v = value.get::<u64>().unwrap_or(800).clamp(200, 10000);
                *self.inner.probe_period_ms.lock() = v;
            }
            21 => {
                let s = value
                    .get::<Option<String>>()
                    .unwrap_or(Some("swrr".to_string()));
                let scheduler = if let Some(s) = s {
                    if s.eq_ignore_ascii_case("drr") {
                        Scheduler::Drr
                    } else {
                        Scheduler::Swrr
                    }
                } else {
                    Scheduler::Swrr
                };
                *self.inner.scheduler.lock() = scheduler;
            }
            22 => {
                let v = value.get::<u32>().unwrap_or(1500).clamp(256, 16384);
                *self.inner.quantum_bytes.lock() = v;
            }
            _ => {}
        }
    }

    fn property(&self, id: usize, _pspec: &glib::ParamSpec) -> glib::Value {
        match id {
            1 => {
                let weights = &self.inner.state.lock().weights;
                let json = serde_json::to_string(weights).unwrap_or_default();
                json.to_value()
            }
            2 => self.inner.rebalance_interval_ms.lock().to_value(),
            3 => {
                let strategy = *self.inner.strategy.lock();
                match strategy {
                    Strategy::Aimd => "aimd".to_value(),
                    Strategy::Ewma => "ewma".to_value(),
                }
            }
            4 => self.inner.caps_any.lock().to_value(),
            5 => self.inner.auto_balance.lock().to_value(),
            6 => self.inner.rist_element.lock().to_value(),
            7 => {
                let weights = &self.inner.state.lock().weights;
                let json = serde_json::to_string(weights).unwrap_or_default();
                json.to_value()
            }
            8 => self.inner.min_hold_ms.lock().to_value(),
            9 => self.inner.switch_threshold.lock().to_value(),
            10 => self.inner.health_warmup_ms.lock().to_value(),
            11 => self.inner.duplicate_keyframes.lock().to_value(),
            12 => self.inner.dup_budget_pps.lock().to_value(),
            13 => self.inner.metrics_export_interval_ms.lock().to_value(),
            14 => self.inner.ewma_rtx_penalty.lock().to_value(),
            15 => self.inner.ewma_rtt_penalty.lock().to_value(),
            16 => self.inner.aimd_rtx_threshold.lock().to_value(),
            17 => self.inner.probe_ratio.lock().to_value(),
            18 => self.inner.max_link_share.lock().to_value(),
            19 => self.inner.probe_boost.lock().to_value(),
            20 => self.inner.probe_period_ms.lock().to_value(),
            21 => {
                let s = *self.inner.scheduler.lock();
                match s {
                    Scheduler::Swrr => "swrr".to_value(),
                    Scheduler::Drr => "drr".to_value(),
                }
            }
            22 => self.inner.quantum_bytes.lock().to_value(),
            _ => "".to_value(),
        }
    }
}

impl GstObjectImpl for DispatcherImpl {}

impl ElementImpl for DispatcherImpl {
    fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
        static ELEMENT_METADATA: Lazy<gst::subclass::ElementMetadata> = Lazy::new(|| {
            gst::subclass::ElementMetadata::new(
                "RIST Dispatcher",
                "Filter/Network",
                "RIST-aware dispatcher with NACK/RTT load balancing",
                "Jake",
            )
        });
        Some(&*ELEMENT_METADATA)
    }

    fn pad_templates() -> &'static [gst::PadTemplate] {
        static PAD_TEMPLATES: Lazy<Vec<gst::PadTemplate>> = Lazy::new(|| {
            let any_caps = gst::Caps::new_any();
            let sink_pad_template = gst::PadTemplate::new(
                "sink",
                gst::PadDirection::Sink,
                gst::PadPresence::Always,
                &any_caps,
            )
            .unwrap();
            let rtp_caps = gst::Caps::builder("application/x-rtp").build();
            let src_pad_template_rtp = gst::PadTemplate::new(
                "src_%u",
                gst::PadDirection::Src,
                gst::PadPresence::Request,
                &rtp_caps,
            )
            .unwrap();
            let src_pad_template_any = gst::PadTemplate::new(
                "src_any_%u",
                gst::PadDirection::Src,
                gst::PadPresence::Request,
                &any_caps,
            )
            .unwrap();
            vec![
                sink_pad_template,
                src_pad_template_rtp,
                src_pad_template_any,
            ]
        });
        PAD_TEMPLATES.as_ref()
    }

    fn request_new_pad(
        &self,
        templ: &gst::PadTemplate,
        _name: Option<&str>,
        _caps: Option<&gst::Caps>,
    ) -> Option<gst::Pad> {
        if templ.direction() != gst::PadDirection::Src {
            return None;
        }
        let mut srcpads = self.inner.srcpads.lock();
        if let Some(requested) = _name {
            if let Some(existing) = srcpads.iter().find(|p| p.name() == requested) {
                return Some(existing.clone());
            }
        }
        let idx = srcpads.len();
        let requested_name = _name.map(|s| s.to_string());
        let existing_names: std::collections::HashSet<String> =
            srcpads.iter().map(|p| p.name().to_string()).collect();
        let pad_name = if let Some(name) = requested_name {
            if !existing_names.contains(&name) {
                name
            } else {
                let mut i = 0usize;
                loop {
                    let c = format!("src_{}", i);
                    if !existing_names.contains(&c) {
                        break c;
                    }
                    i += 1;
                }
            }
        } else {
            let mut i = 0usize;
            loop {
                let c = format!("src_{}", i);
                if !existing_names.contains(&c) {
                    break c;
                }
                i += 1;
            }
        };
        let sinkpad = self.inner.sinkpad.lock().clone();
        let pad = gst::Pad::builder_from_template(templ)
            .name(&pad_name)
            .activatemode_function(move |_pad, _parent, mode, _active| match mode {
                gst::PadMode::Push => Ok(()),
                gst::PadMode::Pull => Err(gst::LoggableError::new(
                    *CAT,
                    glib::bool_error!("Pull mode not supported"),
                )),
                _ => Ok(()),
            })
            .event_function({
                let sinkpad = sinkpad.clone();
                let inner_weak = Arc::downgrade(&self.inner);
                move |_pad, _parent, event| {
                    let event_type = event.type_();
                    if matches!(
                        event_type,
                        gst::EventType::FlushStart | gst::EventType::FlushStop
                    ) {
                        if let Some(inner) = inner_weak.upgrade() {
                            let srcpads = inner.srcpads.lock();
                            let mut all_success = true;
                            for srcpad in srcpads.iter() {
                                if !srcpad.push_event(event.clone()) {
                                    all_success = false;
                                }
                            }
                            drop(srcpads);
                            all_success
                        } else {
                            false
                        }
                    } else if let Some(ref sink) = sinkpad {
                        sink.push_event(event)
                    } else {
                        false
                    }
                }
            })
            .query_function({
                let sinkpad = sinkpad.clone();
                move |_pad, _parent, query| {
                    if let Some(ref sink) = sinkpad {
                        sink.peer_query(query)
                    } else {
                        false
                    }
                }
            })
            .build();
        self.obj().add_pad(&pad).ok()?;
        {
            let state = self.inner.state.lock();
            if let Some(ref e) = state.cached_stream_start {
                pad.push_event(e.clone());
            }
            if let Some(ref e) = state.cached_caps {
                pad.push_event(e.clone());
            }
            if let Some(ref e) = state.cached_segment {
                pad.push_event(e.clone());
            }
            for tag in state.cached_tags.iter() {
                pad.push_event(tag.clone());
            }
        }
        srcpads.push(pad.clone());
        let mut st = self.inner.state.lock();
        if st.weights.len() <= idx {
            st.weights.resize(idx + 1, 1.0);
        }
        while st.swrr_counters.len() < st.weights.len() {
            st.swrr_counters.push(0.0);
        }
        while st.drr_deficits.len() < st.weights.len() {
            let quantum_warm_start = *self.inner.quantum_bytes.lock() as i64;
            st.drr_deficits.push(quantum_warm_start);
        }
        while st.link_health_timers.len() < st.weights.len() {
            st.link_health_timers.push(std::time::Instant::now());
        }
        Some(pad)
    }

    fn release_pad(&self, pad: &gst::Pad) {
        let mut srcpads = self.inner.srcpads.lock();
        if let Some(pos) = srcpads.iter().position(|p| p == pad) {
            self.obj().remove_pad(&srcpads[pos]).ok();
            srcpads.remove(pos);
            let mut state = self.inner.state.lock();
            if pos < state.weights.len() {
                state.weights.remove(pos);
            }
            if pos < state.link_stats.len() {
                state.link_stats.remove(pos);
            }
            if pos < state.swrr_counters.len() {
                state.swrr_counters.remove(pos);
            }
            if pos < state.drr_deficits.len() {
                state.drr_deficits.remove(pos);
            }
            if pos < state.link_health_timers.len() {
                state.link_health_timers.remove(pos);
            }
            if state.drr_ptr >= srcpads.len() && !srcpads.is_empty() {
                state.drr_ptr = srcpads.len() - 1;
            }
            let new_len = srcpads.len();
            if new_len > 0 && state.next_out >= new_len {
                state.next_out = new_len - 1;
            } else if new_len == 0 {
                state.next_out = 0;
            }
        }
    }
}

impl DispatcherImpl {
    pub fn handle_chain(
        inner: &Arc<DispatcherInner>,
        buf: gst::Buffer,
    ) -> Result<gst::FlowSuccess, gst::FlowError> {
        let mut st = inner.state.lock();
        let srcpads = inner.srcpads.lock();
        let srcpads_count = srcpads.len();
        if st.weights.is_empty() {
            st.weights = vec![1.0; srcpads_count];
        } else if st.weights.len() < srcpads_count {
            st.weights.resize(srcpads_count, 1.0);
        } else if st.weights.len() > srcpads_count {
            st.weights.truncate(srcpads_count);
        }
        if srcpads.is_empty() {
            return Err(gst::FlowError::NotLinked);
        }
        while st.swrr_counters.len() < st.weights.len() {
            st.swrr_counters.push(0.0);
        }
        let quantum_warm_start = *inner.quantum_bytes.lock() as i64;
        while st.drr_deficits.len() < st.weights.len() {
            st.drr_deficits.push(quantum_warm_start);
        }
        while st.link_health_timers.len() < st.weights.len() {
            st.link_health_timers.push(std::time::Instant::now());
        }
        let scheduler = *inner.scheduler.lock();
        let (chosen_idx, did_switch) = match scheduler {
            Scheduler::Swrr => {
                let min_hold_ms = *inner.min_hold_ms.lock();
                let switch_threshold = *inner.switch_threshold.lock();
                let health_warmup_ms = *inner.health_warmup_ms.lock();
                let weights = st.weights.clone();
                let current_idx = st.next_out;
                let last_switch = st.last_switch_time;
                let health_timers = st.link_health_timers.clone();
                pick_output_index_swrr_with_hysteresis(
                    &weights,
                    &mut st.swrr_counters,
                    current_idx,
                    last_switch,
                    min_hold_ms,
                    switch_threshold,
                    health_warmup_ms,
                    &health_timers,
                )
            }
            Scheduler::Drr => {
                let base_q = *inner.quantum_bytes.lock() as f64;
                let health_warmup_ms = *inner.health_warmup_ms.lock();
                let weights = st.weights.clone();
                let health_timers = st.link_health_timers.clone();
                let now = std::time::Instant::now();
                let mut adjusted = weights.clone();
                for (i, &t0) in health_timers.iter().enumerate() {
                    if i < adjusted.len() {
                        let ms = now.duration_since(t0).as_millis() as u64;
                        if ms < health_warmup_ms {
                            let f = ms as f64 / health_warmup_ms as f64;
                            adjusted[i] *= 1.0 - 0.5 * (1.0 - f);
                        }
                    }
                }
                let sum: f64 = adjusted.iter().sum();
                let norm = if sum > 0.0 { 1.0 / sum } else { 0.0 };
                if norm > 0.0 {
                    for w in &mut adjusted {
                        *w *= norm;
                    }
                }
                let n = adjusted.len() as f64;
                let mut _quanta: Vec<f64> = adjusted.iter().map(|w| base_q * *w * n).collect();
                for q in &mut _quanta {
                    *q = q.max(256.0);
                }
                let pkt_bytes = buf.size();
                let quantum_bytes = base_q as usize;
                let min_burst_pkts = *inner.min_burst_pkts.lock();
                let link_stats = st.link_stats.clone();
                let mut burst_state = (st.drr_current_burst, st.drr_last_selected);
                let idx = pick_output_index_drr_burst_aware(
                    crate::dispatcher::scheduler::drr::DrrPickParams {
                        pkt_bytes,
                        weights: &adjusted,
                        deficits: &mut st.drr_deficits,
                        quantum_bytes,
                        link_stats: &link_stats,
                        burst_state: &mut burst_state,
                        min_burst_pkts,
                        srcpads: &srcpads,
                    },
                );
                st.drr_current_burst = burst_state.0;
                st.drr_last_selected = burst_state.1;
                let did_switch = idx != st.next_out;
                (idx, did_switch)
            }
        };
        if did_switch {
            st.last_switch_time = Some(std::time::Instant::now());
        }
        st.next_out = chosen_idx;
        drop(st);
        if let Some(outpad) = srcpads.get(chosen_idx) {
            if outpad.is_linked() {
                let should_duplicate = did_switch
                    && *inner.duplicate_keyframes.lock()
                    && crate::dispatcher::duplication::is_keyframe(&buf);
                let can_dup = if should_duplicate {
                    let mut st = inner.state.lock();
                    crate::dispatcher::duplication::can_duplicate_keyframe(inner.as_ref(), &mut st)
                } else {
                    false
                };
                if let Ok(flow) = outpad.push(buf.clone()) {
                    if scheduler == Scheduler::Drr {
                        let pkt_size = buf.size();
                        let base_q = *inner.quantum_bytes.lock() as i64;
                        let mut st2 = inner.state.lock();
                        st2.orig_packets += 1;
                        st2.last_buffer_time = std::time::Instant::now();
                        if chosen_idx < st2.drr_deficits.len() {
                            let new_def = st2.drr_deficits[chosen_idx] - pkt_size as i64;
                            let floor = -4 * base_q;
                            st2.drr_deficits[chosen_idx] = new_def.max(floor);
                        }
                        let srcpads_len = srcpads.len();
                        if srcpads_len > 0 {
                            st2.drr_ptr = (chosen_idx + 1) % srcpads_len;
                        }
                    } else {
                        let mut st2 = inner.state.lock();
                        st2.orig_packets += 1;
                        st2.last_buffer_time = std::time::Instant::now();
                    }
                    if should_duplicate && can_dup && srcpads.len() > 1 {
                        crate::dispatcher::duplication::duplicate_keyframe_to_backup(
                            inner.as_ref(),
                            &srcpads,
                            chosen_idx,
                            &buf,
                        );
                    }
                    return Ok(flow);
                }
            }
        }
        for try_idx in 0..srcpads.len() {
            let idx = (chosen_idx + try_idx + 1) % srcpads.len();
            if let Some(outpad) = srcpads.get(idx) {
                if outpad.is_linked() {
                    match outpad.push(buf.clone()) {
                        Ok(flow) => {
                            if scheduler == Scheduler::Drr {
                                let mut st = inner.state.lock();
                                st.orig_packets += 1;
                                st.last_buffer_time = std::time::Instant::now();
                                if let Some(def) = st.drr_deficits.get_mut(idx) {
                                    let base_q = *inner.quantum_bytes.lock() as i64;
                                    let new_def = *def - buf.size() as i64;
                                    *def = new_def.max(-4 * base_q);
                                }
                                st.drr_ptr = (idx + 1) % srcpads.len();
                            } else {
                                let mut st = inner.state.lock();
                                st.orig_packets += 1;
                                st.last_buffer_time = std::time::Instant::now();
                            }
                            return Ok(flow);
                        }
                        Err(_) => continue,
                    }
                }
            }
        }
        Err(gst::FlowError::NotLinked)
    }

    pub fn handle_sink_event(
        inner: &Arc<DispatcherInner>,
        pad: &gst::Pad,
        parent: Option<&gst::Object>,
        event: gst::Event,
    ) -> bool {
        let event_type = event.type_();
        let is_sticky = matches!(
            event_type,
            gst::EventType::StreamStart
                | gst::EventType::Caps
                | gst::EventType::Segment
                | gst::EventType::Tag
        );
        if is_sticky {
            let mut state = inner.state.lock();
            let srcpads = inner.srcpads.lock();
            match event_type {
                gst::EventType::StreamStart => {
                    state.cached_stream_start = Some(event.clone());
                }
                gst::EventType::Caps => {
                    state.cached_caps = Some(event.clone());
                }
                gst::EventType::Segment => {
                    state.cached_segment = Some(event.clone());
                }
                gst::EventType::Tag => {
                    state.cached_tags.push(event.clone());
                }
                _ => {}
            }
            for srcpad in srcpads.iter() {
                srcpad.push_event(event.clone());
            }
            true
        } else {
            let srcpads = inner.srcpads.lock();
            match event_type {
                gst::EventType::Eos
                | gst::EventType::FlushStart
                | gst::EventType::FlushStop
                | gst::EventType::Reconfigure => {
                    let mut all_success = true;
                    for srcpad in srcpads.iter() {
                        if !srcpad.push_event(event.clone()) {
                            all_success = false;
                        }
                    }
                    all_success
                }
                _ => gst::Pad::event_default(pad, parent, event),
            }
        }
    }

    pub fn handle_sink_query(
        inner: &Arc<DispatcherInner>,
        pad: &gst::Pad,
        parent: Option<&gst::Object>,
        query: &mut gst::QueryRef,
    ) -> bool {
        match query.view_mut() {
            gst::QueryViewMut::Caps(caps_query) => {
                let use_any_caps = *inner.caps_any.lock();
                if use_any_caps {
                    let caps = gst::Caps::new_any();
                    caps_query.set_result(&caps);
                    return true;
                }
                let srcpads = inner.srcpads.lock();
                for srcpad in srcpads.iter() {
                    if srcpad.is_linked() {
                        return srcpad.peer_query(query);
                    }
                }
                let tmpl_caps = pad.pad_template_caps();
                caps_query.set_result(&tmpl_caps);
                true
            }
            _ => {
                let srcpads = inner.srcpads.lock();
                for srcpad in srcpads.iter() {
                    if srcpad.is_linked() {
                        return srcpad.peer_query(query);
                    }
                }
                gst::Pad::query_default::<gst::Pad>(pad, parent, query)
            }
        }
    }

    fn discover_rist_sink_parent(&self) {
        let obj = self.obj();
        let mut parent = obj.parent();
        while let Some(current_parent) = parent.as_ref() {
            let type_name = current_parent.type_().name();
            if type_name == "GstRistSink" {
                if let Ok(rist_element) = current_parent.clone().downcast::<gst::Element>() {
                    *self.inner.rist_element.lock() = Some(rist_element);
                    self.start_stats_polling();
                    return;
                }
            } else if let Ok(el) = current_parent.clone().downcast::<gst::Element>() {
                if let Some(factory) = el.factory() {
                    if factory.name() == "ristsink" {
                        *self.inner.rist_element.lock() = Some(el);
                        self.start_stats_polling();
                        return;
                    }
                }
            }
            parent = current_parent.parent();
        }
    }

    fn start_rebalancer_timer(&self) {
        let auto_balance = *self.inner.auto_balance.lock();
        if !auto_balance {
            return;
        }
        let inner_weak = Arc::downgrade(&self.inner);
        let interval_ms = *self.inner.rebalance_interval_ms.lock();
        if let Some(existing_id) = self.inner.stats_timeout_id.lock().take() {
            existing_id.remove();
        }
        let timeout_id = gst::glib::timeout_add(Duration::from_millis(interval_ms), move || {
            let inner = match inner_weak.upgrade() {
                Some(inner) => inner,
                None => return glib::ControlFlow::Break,
            };
            let need_discover = inner.rist_element.lock().is_none();
            if need_discover {
                if let Some(sinkpad) = inner.sinkpad.lock().as_ref() {
                    if let Some(dispatcher_parent) = sinkpad.parent() {
                        if let Ok(dispatcher_el) =
                            dispatcher_parent.clone().downcast::<gst::Element>()
                        {
                            let mut p = dispatcher_el.parent();
                            while let Some(cur) = p.as_ref() {
                                let tname = cur.type_().name();
                                if tname == "GstRistSink" {
                                    if let Ok(rist_el) = cur.clone().downcast::<gst::Element>() {
                                        *inner.rist_element.lock() = Some(rist_el);
                                        break;
                                    }
                                } else if let Ok(e) = cur.clone().downcast::<gst::Element>() {
                                    if let Some(f) = e.factory() {
                                        if f.name() == "ristsink" {
                                            *inner.rist_element.lock() = Some(e);
                                            break;
                                        }
                                    }
                                }
                                p = cur.parent();
                            }
                        }
                    }
                }
            }
            crate::dispatcher::stats::poll_rist_stats_and_update_weights(&inner);
            glib::ControlFlow::Continue
        });
        *self.inner.stats_timeout_id.lock() = Some(timeout_id);
    }

    fn stop_rebalancer_timer(&self) {
        if let Some(existing_id) = self.inner.stats_timeout_id.lock().take() {
            existing_id.remove();
        }
    }

    fn start_flow_watchdog(&self) {
        if self.inner.flow_watchdog_id.lock().is_some() {
            return;
        }
        let inner_weak = Arc::downgrade(&self.inner);
        let id = gst::glib::timeout_add(Duration::from_millis(200), move || {
            if let Some(inner) = inner_weak.upgrade() {
                let mut st = inner.state.lock();
                let now = std::time::Instant::now();
                // No-op check removed to avoid empty conditional
                st.last_flow_check_packets = st.orig_packets;
                st.last_flow_check_time = now;
            }
            glib::ControlFlow::Continue
        });
        *self.inner.flow_watchdog_id.lock() = Some(id);
    }

    fn start_stats_polling(&self) {
        self.start_rebalancer_timer();
    }

    pub fn register(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
        gst::Element::register(
            Some(plugin),
            "ristdispatcher",
            gst::Rank::NONE,
            Dispatcher::static_type(),
        )
    }
    pub fn register_static() -> Result<(), glib::BoolError> {
        gst::Element::register(
            None,
            "ristdispatcher",
            gst::Rank::NONE,
            Dispatcher::static_type(),
        )
    }
}

pub fn register(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    DispatcherImpl::register(plugin)
}
pub fn register_static() -> Result<(), glib::BoolError> {
    DispatcherImpl::register_static()
}
