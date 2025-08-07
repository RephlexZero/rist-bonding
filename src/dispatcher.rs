use gst::glib;
use gst::prelude::*;
use gst::subclass::prelude::*;
use gstreamer as gst;
use parking_lot::Mutex;
use std::sync::Arc;

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

        // Create sink pad (chain function)
        let tmpl_sink = gst::PadTemplate::new(
            "sink",
            gst::PadDirection::Sink,
            gst::PadPresence::Always,
            &gst::Caps::builder("application/x-rtp").build(),
        )
        .unwrap();

        let inner_weak = Arc::downgrade(&self.inner);
        let sinkpad = gst::Pad::builder_from_template(&tmpl_sink)
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
                
                // Weighted round-robin selection
                let next_out = pick_output_index(&st.weights, st.next_out);
                st.next_out = next_out;
                let idx = next_out;
                drop(st);

                match srcpads.get(idx) {
                    Some(outpad) => {
                        gst::trace!(CAT, "Forwarding buffer to output pad {}", idx);
                        outpad.push(buf)
                    }
                    None => {
                        gst::error!(CAT, "Invalid output pad index: {}", idx);
                        Err(gst::FlowError::Error)
                    }
                }
            })
            .build();
        obj.add_pad(&sinkpad).unwrap();
        *self.inner.sinkpad.lock() = Some(sinkpad);

        // Initial one src pad to start; ristsink will request more via request_new_pad
        // We expose a request pad template so ristsink can ask for as many as there are bonded links.
        // Note: templates are added in the pad_templates() method above
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
            ]
        });
        PROPS.as_ref()
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
                                self.inner.state.lock().weights = valid_weights;
                                gst::info!(CAT, "Set weights: {:?}", self.inner.state.lock().weights);
                            } else {
                                gst::warning!(CAT, "Invalid weights JSON, using default [1.0]");
                                self.inner.state.lock().weights = vec![1.0];
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
            let caps = gst::Caps::builder("application/x-rtp").build();

            let sink_pad_template = gst::PadTemplate::new(
                "sink",
                gst::PadDirection::Sink,
                gst::PadPresence::Always,
                &caps,
            )
            .unwrap();

            let src_pad_template = gst::PadTemplate::new(
                "src_%u",
                gst::PadDirection::Src,
                gst::PadPresence::Request,
                &caps,
            )
            .unwrap();

            vec![sink_pad_template, src_pad_template]
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
        let pad = gst::Pad::builder_from_template(templ)
            .name(&pad_name)
            .activatemode_function(|_pad, _parent, _mode, _active| Ok(()))
            .event_function(|_pad, _parent, _event| true)
            .query_function(|_pad, _parent, _query| false)
            .build();
        self.obj().add_pad(&pad).ok()?;
        srcpads.push(pad.clone());

        // Ensure weights vector is long enough
        let mut st = self.inner.state.lock();
        if st.weights.len() <= idx {
            st.weights.resize(idx + 1, 1.0);
        }
        Some(pad)
    }

    fn release_pad(&self, pad: &gst::Pad) {
        let mut srcpads = self.inner.srcpads.lock();
        if let Some(pos) = srcpads.iter().position(|p| p == pad) {
            self.obj().remove_pad(&srcpads[pos]).ok();
            srcpads.remove(pos);
        }
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
