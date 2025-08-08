use gst::glib;
use gst::prelude::*;
use gst::subclass::prelude::*;
use gstreamer as gst;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Duration;

// Logging category
use once_cell::sync::Lazy;
static CAT: Lazy<gst::DebugCategory> = Lazy::new(|| {
    gst::DebugCategory::new(
        "ristdispatcher",
        gst::DebugColorFlags::empty(),
        Some("RIST Dispatcher"),
    )
});

// Element: ristdispatcher
// One sink pad (chain-based), N request src pads: src_%u
// For use as ristsink "dispatcher"; ristsink will request src pads for bonded sessions.

// Properties
// - weights: optional JSON array of initial link weights [w0, w1, ...]
// - rebalance-interval-ms: how often to recompute weights from stats
// - strategy: "aimd" | "ewma" (affects weight updates)

#[derive(Default)]
pub struct State {
    // Selected output index at the time a packet arrives
    next_out: usize,
    // Runtime-computed weights per link
    weights: Vec<f64>,
    // Rolling stats we compute from upstream NACK/RTT messages (fed by ristsink session stats)
    // ristsink calls into the dispatcher via sticky element messages we watch on src pads.
    // Cached sticky events to replay on new src pads
    cached_stream_start: Option<gst::Event>,
    cached_caps: Option<gst::Event>,
    cached_segment: Option<gst::Event>,
    cached_tags: Vec<gst::Event>,
    // Per-link stats tracking for EWMA calculation
    link_stats: Vec<LinkStats>,
}

#[derive(Debug, Clone)]
pub struct LinkStats {
    // Previous measurements for delta calculation
    prev_sent_original: u64,
    prev_sent_retransmitted: u64,
    prev_timestamp: std::time::Instant,
    // EWMA values
    ewma_goodput: f64,     // packets per second
    ewma_rtx_rate: f64,    // retransmission rate (0.0 to 1.0)
    ewma_rtt: f64,         // round trip time in ms
    // EWMA smoothing factors
    alpha: f64,            // smoothing factor for rates (0.2-0.3)
}

impl Default for LinkStats {
    fn default() -> Self {
        Self {
            prev_sent_original: 0,
            prev_sent_retransmitted: 0,
            prev_timestamp: std::time::Instant::now(),
            ewma_goodput: 0.0,
            ewma_rtx_rate: 0.0,
            ewma_rtt: 50.0, // reasonable default RTT
            alpha: 0.25,    // default smoothing factor
        }
    }
}

#[derive(Default)]
pub struct DispatcherInner {
    state: Mutex<State>,
    // Pads
    sinkpad: Mutex<Option<gst::Pad>>,
    srcpads: Mutex<Vec<gst::Pad>>,
    // Config
    rebalance_interval_ms: Mutex<u64>,
    strategy: Mutex<Strategy>,
    caps_any: Mutex<bool>,
    auto_balance: Mutex<bool>,
    // RIST stats polling
    rist_element: Mutex<Option<gst::Element>>,
    stats_timeout_id: Mutex<Option<glib::SourceId>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Strategy {
    Aimd,
    Ewma,
}
impl Default for Strategy {
    fn default() -> Self {
        Strategy::Ewma
    }
}

// Public wrapper type so GStreamer can instantiate it
glib::wrapper! {
    pub struct Dispatcher(ObjectSubclass<DispatcherImpl>) @extends gst::Element, gst::Object;
}

// Subclass implementation
#[derive(Default)]
pub struct DispatcherImpl {
    inner: Arc<DispatcherInner>,
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

        // Create sink pad - use the single sink template
        // Actual caps are controlled by the caps-any property via template caps
        
        // Get the single sink template and create pad from it
        let sink_template = Self::pad_templates()
            .iter()
            .find(|tmpl| tmpl.name() == "sink")
            .unwrap();

        let inner_weak = Arc::downgrade(&self.inner);
        let inner_weak_query = Arc::downgrade(&self.inner);
        let sinkpad = gst::Pad::builder_from_template(sink_template)
            .name("sink")
            .chain_function(move |_pad, _parent, buf| {
                let inner = match inner_weak.upgrade() {
                    Some(inner) => inner,
                    None => {
                        gst::error!(CAT, "Failed to upgrade inner reference");
                        return Err(gst::FlowError::Flushing);
                    }
                };

                // Choose output
                let mut st = inner.state.lock();
                let srcpads = inner.srcpads.lock();

                if st.weights.is_empty() {
                    gst::warning!(CAT, "No weights configured, using default");
                    st.weights.push(1.0);
                }

                if srcpads.is_empty() {
                    gst::warning!(CAT, "No source pads available");
                    drop(st);
                    drop(srcpads);
                    return Err(gst::FlowError::NotNegotiated);
                }

                // Weighted round-robin selection with fallback
                let chosen_idx = pick_output_index(&st.weights, st.next_out);
                st.next_out = chosen_idx;
                drop(st);

                // Try chosen pad first, then fallback to others if not linked
                for try_idx in 0..srcpads.len() {
                    let idx = (chosen_idx + try_idx) % srcpads.len();
                    if let Some(outpad) = srcpads.get(idx) {
                        if outpad.is_linked() {
                            gst::trace!(CAT, "Forwarding buffer to output pad {}", idx);
                            match outpad.push(buf.clone()) {
                                Ok(flow) => return Ok(flow),
                                Err(gst::FlowError::NotLinked) => continue,
                                Err(e) => return Err(e),
                            }
                        }
                    }
                }

                gst::warning!(CAT, "No linked output pads available");
                Err(gst::FlowError::NotLinked)
            })
            .event_function({
                let inner_weak = Arc::downgrade(&self.inner);
                move |_pad, _parent, event| {
                    let inner = match inner_weak.upgrade() {
                        Some(inner) => inner,
                        None => {
                            gst::error!(CAT, "Failed to upgrade inner reference in event function");
                            return false;
                        }
                    };

                    // Handle sticky events
                    let event_type = event.type_();
                    let is_sticky = matches!(event_type, 
                        gst::EventType::StreamStart | 
                        gst::EventType::Caps | 
                        gst::EventType::Segment | 
                        gst::EventType::Tag
                    );

                    if is_sticky {
                        let mut state = inner.state.lock();
                        let srcpads = inner.srcpads.lock();

                        // Cache the sticky event
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
                                // For tags, we might want to accumulate multiple ones
                                state.cached_tags.push(event.clone());
                            }
                            _ => {}
                        }

                        // Fan-out to all existing src pads
                        for srcpad in srcpads.iter() {
                            if !srcpad.push_event(event.clone()) {
                                gst::warning!(CAT, "Failed to push sticky event to src pad {}", srcpad.name());
                            }
                        }

                        drop(state);
                        drop(srcpads);
                        true
                    } else {
                        // For non-sticky events, use default handling (pass downstream or handle)
                        true
                    }
                }
            })
            .query_function({
                let inner_weak = Arc::downgrade(&self.inner);
                move |_pad, _parent, query| {
                    let inner = match inner_weak.upgrade() {
                        Some(inner) => inner,
                        None => {
                            gst::error!(CAT, "Failed to upgrade inner reference in query function");
                            return false;
                        }
                    };

                    // Forward downstream queries to a linked src pad
                    let srcpads = inner.srcpads.lock();
                    
                    // Find the first linked src pad for forwarding
                    for srcpad in srcpads.iter() {
                        if srcpad.is_linked() {
                            gst::trace!(CAT, "Forwarding sink query {:?} to src pad {}", query.type_(), srcpad.name());
                            return srcpad.peer_query(query);
                        }
                    }
                    
                    // If no linked pads, handle conservatively
                    match query.view() {
                        gst::QueryView::Caps(_) => {
                            // Let GStreamer handle caps queries with our pad template
                            false
                        }
                        gst::QueryView::AcceptCaps(_) => {
                            // Accept caps conservatively
                            false
                        }
                        gst::QueryView::Latency(_) => {
                            // Return a reasonable latency estimate if we can't forward
                            false
                        }
                        gst::QueryView::Allocation(_) => {
                            // Let downstream handle allocation
                            false
                        }
                        _ => false
                    }
                }
            })
            .query_function({
                move |_pad, _parent, query| {
                    let inner = match inner_weak_query.upgrade() {
                        Some(inner) => inner,
                        None => return false,
                    };
                    
                    // Handle caps query based on caps-any property
                    match query.view_mut() {
                        gst::QueryViewMut::Caps(caps_query) => {
                            let use_any_caps = *inner.caps_any.lock();
                            let caps = if use_any_caps {
                                gst::Caps::new_any()
                            } else {
                                gst::Caps::builder("application/x-rtp").build()
                            };
                            caps_query.set_result(&caps);
                            true
                        }
                        _ => {
                            // Default handling for other queries
                            gst::Pad::query_default(_pad, _parent, query)
                        }
                    }
                }
            })
            .build();
        obj.add_pad(&sinkpad).unwrap();
        *self.inner.sinkpad.lock() = Some(sinkpad);

        // Start the rebalancer timer even without RIST element
        // This provides basic weight adjustment capabilities
        self.start_rebalancer_timer();
    }

    fn properties() -> &'static [glib::ParamSpec] {
        use once_cell::sync::Lazy;
        static PROPS: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
            vec![
                glib::ParamSpecString::builder("weights")
                    .nick("Initial weights JSON")
                    .blurb("JSON array of initial link weights, e.g., [1.0, 1.0, 2.0]")
                    .default_value(Some("[1.0]"))
                    .build(),
                glib::ParamSpecUInt64::builder("rebalance-interval-ms")
                    .nick("Rebalance interval (ms)")
                    .blurb("How often to recompute weights from statistics in milliseconds")
                    .minimum(100)
                    .maximum(10000)
                    .default_value(500)
                    .build(),
                glib::ParamSpecString::builder("strategy")
                    .nick("Load balancing strategy")
                    .blurb("Strategy for weight updates: 'aimd' or 'ewma'")
                    .default_value(Some("ewma"))
                    .build(),
                glib::ParamSpecBoolean::builder("caps-any")
                    .nick("Use ANY caps")
                    .blurb("Use ANY caps instead of application/x-rtp for broader compatibility")
                    .default_value(false)
                    .build(),
                glib::ParamSpecBoolean::builder("auto-balance")
                    .nick("Auto balance")
                    .blurb("Enable automatic rebalancing timer (disable when using external controller like dynbitrate)")
                    .default_value(true)
                    .build(),
                glib::ParamSpecObject::builder::<gst::Element>("rist")
                    .nick("RIST element")
                    .blurb("The RIST sink element to read statistics from for adaptive weighting")
                    .build(),
                glib::ParamSpecString::builder("current-weights")
                    .nick("Current weights (readonly)")
                    .blurb("Current weight values as JSON array - readonly for monitoring")
                    .flags(glib::ParamFlags::READABLE)
                    .build(),
            ]
        });
        PROPS.as_ref()
    }
    
    fn signals() -> &'static [glib::subclass::Signal] {
        use once_cell::sync::Lazy;
        static SIGNALS: Lazy<Vec<glib::subclass::Signal>> = Lazy::new(|| {
            vec![
                glib::subclass::Signal::builder("weights-changed")
                    .param_types([String::static_type()])
                    .build(),
            ]
        });
        SIGNALS.as_ref()
    }

    fn set_property(&self, id: usize, value: &glib::Value, _pspec: &glib::ParamSpec) {
        match id {
            0 => {
                // weights - parse JSON array
                if let Ok(Some(s)) = value.get::<Option<String>>() {
                    match serde_json::from_str::<Vec<f64>>(&s) {
                        Ok(weights) => {
                            // Validate weights
                            let valid_weights: Vec<f64> = weights
                                .into_iter()
                                .map(|w| if w.is_finite() && w >= 0.0 { w } else { 1.0 })
                                .collect();

                            if !valid_weights.is_empty() {
                                self.inner.state.lock().weights = valid_weights.clone();
                                gst::info!(
                                    CAT,
                                    "Set weights: {:?}",
                                    self.inner.state.lock().weights
                                );
                                
                                // Emit weights-changed signal for external updates
                                let weights_json = serde_json::to_string(&valid_weights).unwrap_or_default();
                                self.obj().emit_by_name::<()>("weights-changed", &[&weights_json]);
                            } else {
                                gst::warning!(CAT, "Invalid weights JSON, using default [1.0]");
                                self.inner.state.lock().weights = vec![1.0];
                                self.obj().emit_by_name::<()>("weights-changed", &[&"[1.0]".to_string()]);
                            }
                        }
                        Err(e) => {
                            gst::warning!(CAT, "Failed to parse weights JSON: {}", e);
                        }
                    }
                }
            }
            1 => {
                let interval = value.get::<u64>().unwrap_or(500).max(100).min(10000);
                *self.inner.rebalance_interval_ms.lock() = interval;
                gst::debug!(CAT, "Set rebalance interval: {} ms", interval);
                
                // Restart timer immediately with new interval
                self.start_rebalancer_timer();
            }
            2 => {
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
                gst::debug!(CAT, "Set strategy: {:?}", strategy);
            }
            3 => {
                let caps_any = value.get::<bool>().unwrap_or(false);
                *self.inner.caps_any.lock() = caps_any;
                gst::debug!(CAT, "Set caps-any: {}", caps_any);
            }
            4 => {
                let auto_balance = value.get::<bool>().unwrap_or(true);
                *self.inner.auto_balance.lock() = auto_balance;
                gst::debug!(CAT, "Set auto-balance: {}", auto_balance);
                
                // Start or stop timer based on auto-balance setting
                if auto_balance {
                    self.start_rebalancer_timer();
                } else {
                    self.stop_rebalancer_timer();
                }
            }
            5 => {
                let rist = value.get::<Option<gst::Element>>().ok().flatten();
                *self.inner.rist_element.lock() = rist.clone();
                gst::debug!(CAT, "Set RIST element: {:?}", rist.is_some());
                
                // Start stats polling if RIST element is set
                if rist.is_some() {
                    self.start_stats_polling();
                }
            }
            _ => {}
        }
    }

    fn property(&self, id: usize, _pspec: &glib::ParamSpec) -> glib::Value {
        match id {
            0 => {
                let weights = &self.inner.state.lock().weights;
                let json = serde_json::to_string(weights).unwrap_or_default();
                json.to_value()
            }
            1 => self.inner.rebalance_interval_ms.lock().to_value(),
            2 => {
                let strategy = *self.inner.strategy.lock();
                match strategy {
                    Strategy::Aimd => "aimd".to_value(),
                    Strategy::Ewma => "ewma".to_value(),
                }
            }
            3 => self.inner.caps_any.lock().to_value(),
            4 => self.inner.auto_balance.lock().to_value(),
            5 => self.inner.rist_element.lock().to_value(),
            6 => {
                // current-weights (readonly)
                let weights = &self.inner.state.lock().weights;
                let json = serde_json::to_string(weights).unwrap_or_default();
                json.to_value()
            }
            _ => {
                // Return a safe default value for unknown properties
                "".to_value()
            }
        }
    }
}

impl GstObjectImpl for DispatcherImpl {}

impl ElementImpl for DispatcherImpl {
    fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
        use once_cell::sync::Lazy;
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
        use once_cell::sync::Lazy;
        static PAD_TEMPLATES: Lazy<Vec<gst::PadTemplate>> = Lazy::new(|| {
            // Single sink template with ANY caps - actual caps controlled by caps-any property
            let any_caps = gst::Caps::new_any();

            let sink_pad_template = gst::PadTemplate::new(
                "sink",
                gst::PadDirection::Sink,
                gst::PadPresence::Always,
                &any_caps,
            )
            .unwrap();

            // Request src templates for both RTP and ANY caps
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

            vec![sink_pad_template, src_pad_template_rtp, src_pad_template_any]
        });
        PAD_TEMPLATES.as_ref()
    }

    fn request_new_pad(
        &self,
        templ: &gst::PadTemplate,
        name: Option<&str>,
        _caps: Option<&gst::Caps>,
    ) -> Option<gst::Pad> {
        if templ.direction() != gst::PadDirection::Src {
            return None;
        }

        let mut srcpads = self.inner.srcpads.lock();
        let idx = srcpads.len();
        let pad_name = name
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("src_{}", idx));

        // Get sink pad for upstream forwarding
        let sinkpad = self.inner.sinkpad.lock().clone();

        let pad = gst::Pad::builder_from_template(templ)
            .name(&pad_name)
            .activatemode_function(move |pad, _parent, mode, active| {
                match mode {
                    gst::PadMode::Push => {
                        gst::debug!(CAT, "Activating pad {} in push mode: {}", pad.name(), active);
                        Ok(())
                    }
                    gst::PadMode::Pull => {
                        gst::debug!(CAT, "Pull mode not supported for pad {}", pad.name());
                        Err(gst::LoggableError::new(
                            *CAT,
                            glib::bool_error!("Pull mode not supported")
                        ))
                    }
                    _ => Ok(())
                }
            })
            .event_function({
                let sinkpad = sinkpad.clone();
                move |_pad, _parent, event| {
                    // Forward upstream events to sink pad
                    if let Some(ref sink) = sinkpad {
                        gst::trace!(CAT, "Forwarding upstream event {:?} to sink pad", event.type_());
                        sink.push_event(event)
                    } else {
                        gst::warning!(CAT, "No sink pad available for upstream event forwarding");
                        false
                    }
                }
            })
            .query_function({
                let sinkpad = sinkpad.clone();
                move |_pad, _parent, query| {
                    // Forward upstream queries to sink pad
                    if let Some(ref sink) = sinkpad {
                        gst::trace!(CAT, "Forwarding upstream query {:?} to sink pad", query.type_());
                        sink.peer_query(query)
                    } else {
                        gst::warning!(CAT, "No sink pad available for upstream query forwarding");
                        false
                    }
                }
            })
            .build();

        self.obj().add_pad(&pad).ok()?;
        
        // Replay cached sticky events to the new pad
        {
            let state = self.inner.state.lock();
            
            // Replay in correct order: stream-start → caps → segment → tags
            if let Some(ref stream_start) = state.cached_stream_start {
                if !pad.push_event(stream_start.clone()) {
                    gst::warning!(CAT, "Failed to replay STREAM_START event to new src pad");
                }
            }
            
            if let Some(ref caps) = state.cached_caps {
                if !pad.push_event(caps.clone()) {
                    gst::warning!(CAT, "Failed to replay CAPS event to new src pad");
                }
            }
            
            if let Some(ref segment) = state.cached_segment {
                if !pad.push_event(segment.clone()) {
                    gst::warning!(CAT, "Failed to replay SEGMENT event to new src pad");
                }
            }
            
            // Replay all cached tag events
            for tag_event in &state.cached_tags {
                if !pad.push_event(tag_event.clone()) {
                    gst::warning!(CAT, "Failed to replay TAG event to new src pad");
                }
            }
        }
        
        srcpads.push(pad.clone());

        // Ensure weights vector is long enough
        let mut st = self.inner.state.lock();
        if st.weights.len() <= idx {
            st.weights.resize(idx + 1, 1.0);
        }
        
        gst::info!(CAT, "Created new src pad '{}' (index {})", pad_name, idx);
        Some(pad)
    }

    fn release_pad(&self, pad: &gst::Pad) {
        let mut srcpads = self.inner.srcpads.lock();
        if let Some(pos) = srcpads.iter().position(|p| p == pad) {
            self.obj().remove_pad(&srcpads[pos]).ok();
            srcpads.remove(pos);
            
            // Clean up corresponding weights and stats
            let mut state = self.inner.state.lock();
            if pos < state.weights.len() {
                state.weights.remove(pos);
            }
            if pos < state.link_stats.len() {
                state.link_stats.remove(pos);
            }
            
            // Fix next_out if it points past the end
            let new_len = srcpads.len();
            if new_len > 0 && state.next_out >= new_len {
                state.next_out = new_len - 1;
                gst::debug!(CAT, "Adjusted next_out from {} to {}", state.next_out + new_len, state.next_out);
            } else if new_len == 0 {
                state.next_out = 0;
            }
            
            gst::info!(CAT, "Released src pad at index {}, cleaned up weights and stats, {} pads remaining", pos, new_len);
        }
    }
}

impl DispatcherImpl {
    fn start_rebalancer_timer(&self) {
        // Check auto_balance setting
        let auto_balance = *self.inner.auto_balance.lock();
        if !auto_balance {
            gst::debug!(CAT, "Auto-balance disabled, not starting rebalancer timer");
            return;
        }
        
        let inner_weak = Arc::downgrade(&self.inner);
        let interval_ms = *self.inner.rebalance_interval_ms.lock();
        
        // Stop any existing polling
        if let Some(existing_id) = self.inner.stats_timeout_id.lock().take() {
            existing_id.remove();
        }
        
        let timeout_id = gst::glib::timeout_add_local(Duration::from_millis(interval_ms), move || {
            let inner = match inner_weak.upgrade() {
                Some(inner) => inner,
                None => return glib::ControlFlow::Break,
            };

            // Poll RIST stats and update weights
            Self::poll_rist_stats_and_update_weights(&inner);
            
            glib::ControlFlow::Continue
        });
        
        *self.inner.stats_timeout_id.lock() = Some(timeout_id);
        gst::debug!(CAT, "Started rebalancer timer with interval {} ms", interval_ms);
    }

    fn stop_rebalancer_timer(&self) {
        if let Some(existing_id) = self.inner.stats_timeout_id.lock().take() {
            existing_id.remove();
            gst::debug!(CAT, "Stopped rebalancer timer");
        }
    }

    fn start_stats_polling(&self) {
        // Just restart the timer with current settings
        self.start_rebalancer_timer();
    }
    
    fn poll_rist_stats_and_update_weights(inner: &DispatcherInner) {
        let rist_element = inner.rist_element.lock().clone();
        if let Some(rist) = rist_element {
            // Get stats from ristsink "stats" property -> GstStructure
            let stats_value: glib::Value = rist.property("stats");
            if let Ok(Some(structure)) = stats_value.get::<Option<gst::Structure>>() {
                gst::trace!(CAT, "Got RIST stats structure: {}", structure.to_string());
                Self::update_weights_from_stats(inner, &structure);
            }
        }
    }
    
    fn update_weights_from_stats(inner: &DispatcherInner, stats: &gst::Structure) {
        let strategy = *inner.strategy.lock();
        let mut state = inner.state.lock();
        let now = std::time::Instant::now();
        
        // Ensure we have enough link stats entries
        let num_links = state.weights.len();
        while state.link_stats.len() < num_links {
            state.link_stats.push(LinkStats::default());
        }
        
        // Process per-session stats (each bonded link has its own session)
        for (link_idx, link_stats) in state.link_stats.iter_mut().enumerate() {
            // Try to get per-session stats from the structure
            // The actual field names depend on ristsink implementation
            let session_key = format!("session-{}", link_idx);
            
            if let Ok(sent_original) = stats.get::<u64>(&format!("{}.sent-original-packets", session_key))
                .or_else(|_| stats.get::<u64>("sent-original-packets")) {
                
                let sent_retrans = stats.get::<u64>(&format!("{}.sent-retransmitted-packets", session_key))
                    .or_else(|_| stats.get::<u64>("sent-retransmitted-packets"))
                    .unwrap_or(0);
                
                let rtt_ms = stats.get::<f64>(&format!("{}.round-trip-time", session_key))
                    .or_else(|_| stats.get::<f64>("round-trip-time"))
                    .unwrap_or(50.0);
                
                // Calculate deltas
                let delta_time = now.duration_since(link_stats.prev_timestamp).as_secs_f64();
                if delta_time > 0.1 { // At least 100ms since last update
                    let delta_original = sent_original.saturating_sub(link_stats.prev_sent_original);
                    let delta_retrans = sent_retrans.saturating_sub(link_stats.prev_sent_retransmitted);
                    
                    // Update EWMA values
                    let goodput = delta_original as f64 / delta_time; // packets/sec
                    let rtx_rate = if delta_original > 0 { 
                        delta_retrans as f64 / (delta_original + delta_retrans) as f64 
                    } else { 
                        0.0 
                    };
                    
                    link_stats.ewma_goodput = link_stats.alpha * goodput + (1.0 - link_stats.alpha) * link_stats.ewma_goodput;
                    link_stats.ewma_rtx_rate = link_stats.alpha * rtx_rate + (1.0 - link_stats.alpha) * link_stats.ewma_rtx_rate;
                    link_stats.ewma_rtt = link_stats.alpha * rtt_ms + (1.0 - link_stats.alpha) * link_stats.ewma_rtt;
                    
                    // Update previous values
                    link_stats.prev_sent_original = sent_original;
                    link_stats.prev_sent_retransmitted = sent_retrans;
                    link_stats.prev_timestamp = now;
                }
            }
        }
        
        // Calculate new weights based on strategy
        let weights_changed = match strategy {
            Strategy::Ewma => {
                Self::calculate_ewma_weights(&mut state)
            }
            Strategy::Aimd => {
                Self::calculate_aimd_weights(&mut state)
            }
        };
        
        // Emit signal if weights changed
        if weights_changed {
            let weights_json = serde_json::to_string(&state.weights).unwrap_or_default();
            drop(state); // Release lock before emitting signal
            
            // Try to get the dispatcher object and emit signal
            if let Some(sinkpad) = inner.sinkpad.lock().as_ref() {
                if let Some(parent) = sinkpad.parent() {
                    if let Ok(dispatcher) = parent.downcast::<Dispatcher>() {
                        dispatcher.emit_by_name::<()>("weights-changed", &[&weights_json]);
                    }
                }
            }
        }
    }
    
    fn calculate_ewma_weights(state: &mut State) -> bool {
        // EWMA goodput strategy from the roadmap
        let mut new_weights = vec![0.0; state.weights.len()];
        let mut total_weight = 0.0;
        let mut changed = false;
        
        for (i, stats) in state.link_stats.iter().enumerate() {
            if i >= new_weights.len() { break; }
            
            // Base weight from goodput
            let mut weight = stats.ewma_goodput.max(0.1); // minimum floor
            
            // Penalty for loss: scale by 1 / (1 + α × RTX_rate)
            let alpha_rtx = 0.1; // penalty coefficient from roadmap
            weight *= 1.0 / (1.0 + alpha_rtx * stats.ewma_rtx_rate);
            
            // Optional RTT normalization (prefer lower RTT)
            let beta_rtt = 0.05; // RTT penalty coefficient
            let normalized_rtt = (stats.ewma_rtt / 100.0).max(0.1); // normalize to ~100ms baseline
            weight /= 1.0 + beta_rtt * normalized_rtt;
            
            // Weight floor to avoid starvation
            weight = weight.max(0.05);
            
            new_weights[i] = weight;
            total_weight += weight;
        }
        
        // Normalize weights to sum to 1
        if total_weight > 0.0 {
            for w in &mut new_weights {
                *w /= total_weight;
            }
            
            // Check if weights actually changed significantly
            for (old, new) in state.weights.iter().zip(new_weights.iter()) {
                if (old - new).abs() > 0.01 { // 1% threshold
                    changed = true;
                    break;
                }
            }
            
            if changed {
                state.weights = new_weights;
                gst::debug!(CAT, "Updated EWMA weights: {:?}", state.weights);
            }
        }
        
        changed
    }
    
    fn calculate_aimd_weights(state: &mut State) -> bool {
        // AIMD per-link strategy (TCP-like fairness)
        let rtx_threshold = 0.05; // 5% RTX rate threshold
        let additive_increase = 0.1;
        let multiplicative_decrease = 0.5;
        let mut changed = false;
        
        let old_weights = state.weights.clone();
        
        for (i, stats) in state.link_stats.iter().enumerate() {
            if i >= state.weights.len() { break; }
            
            let current_weight = state.weights[i];
            
            if stats.ewma_rtx_rate < rtx_threshold {
                // Additively increase
                state.weights[i] = (current_weight + additive_increase).min(2.0);
            } else {
                // Multiplicatively decrease
                state.weights[i] = (current_weight * multiplicative_decrease).max(0.05);
            }
        }
        
        // Normalize
        let total: f64 = state.weights.iter().sum();
        if total > 0.0 {
            for w in &mut state.weights {
                *w /= total;
            }
        }
        
        // Check if weights changed significantly
        for (old, new) in old_weights.iter().zip(state.weights.iter()) {
            if (old - new).abs() > 0.01 { // 1% threshold
                changed = true;
                break;
            }
        }
        
        if changed {
            gst::debug!(CAT, "Updated AIMD weights: {:?}", state.weights);
        }
        
        changed
    }
}

// Weighted selection with bounds checking and fallback
fn pick_output_index(weights: &[f64], prev: usize) -> usize {
    if weights.is_empty() {
        gst::warning!(CAT, "Empty weights array, using index 0");
        return 0;
    }

    let n = weights.len();
    let mut best = 0usize;
    let mut best_score = f64::NEG_INFINITY;

    for (i, &w) in weights.iter().enumerate() {
        if !w.is_finite() || w < 0.0 {
            gst::warning!(CAT, "Invalid weight at index {}: {}", i, w);
            continue;
        }

        // Rotate priority to prevent sticky choice
        let rotation_penalty = ((i + n - (prev % n)) % n) as f64 * 1e-6;
        let score = w - rotation_penalty;

        if score > best_score {
            best = i;
            best_score = score;
        }
    }

    // Ensure we return a valid index
    best.min(n.saturating_sub(1))
}

pub fn register(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    gst::Element::register(
        Some(plugin),
        "ristdispatcher",
        gst::Rank::NONE,
        Dispatcher::static_type(),
    )
}
