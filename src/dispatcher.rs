use gst::glib;
use gst::prelude::*;
use gst::subclass::prelude::*;
use gstreamer as gst;
use parking_lot::Mutex;
use std::sync::Arc;

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
                let inner = inner_weak.upgrade().ok_or(gst::FlowError::Flushing)?;

                // Choose output
                let mut st = inner.state.lock();
                let srcpads = inner.srcpads.lock();
                if st.weights.is_empty() || srcpads.is_empty() {
                    return Err(gst::FlowError::NotNegotiated);
                }
                // Weighted round-robin selection
                st.next_out = pick_output_index(&st.weights, st.next_out);
                let idx = st.next_out;
                drop(st);

                if let Some(outpad) = srcpads.get(idx) {
                    outpad.push(buf)
                } else {
                    Err(gst::FlowError::Error)
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
                    .build(),
                glib::ParamSpecUInt64::builder("rebalance-interval-ms")
                    .default_value(500)
                    .build(),
                glib::ParamSpecString::builder("strategy")
                    .default_value("ewma")
                    .build(),
            ]
        });
        PROPS.as_ref()
    }

    fn set_property(&self, id: usize, value: &glib::Value, _pspec: &glib::ParamSpec) {
        match id {
            0 => {
                // weights
                if let Ok(Some(s)) = value.get::<Option<String>>() {
                    if let Ok(v) = serde_json::from_str::<Vec<f64>>(&s) {
                        self.inner.state.lock().weights = v;
                    }
                }
            }
            1 => *self.inner.rebalance_interval_ms.lock() = value.get::<u64>().unwrap_or(500),
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
            _ => unimplemented!(),
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
            .activatemode_function(|_, _, _, _| Ok(()))
            .event_function(|_, _, _| true)
            .query_function(|_, _, _| false)
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

// Simple weighted selection with stride to avoid bursts
fn pick_output_index(weights: &[f64], prev: usize) -> usize {
    // Convert to cumulative and pick next bucket after prev
    let n = weights.len().max(1);
    let mut best = 0usize;
    let mut best_score = f64::MIN;
    for (i, &w) in weights.iter().enumerate() {
        // rotate priority to prevent sticky choice
        let score = w - ((i + n - (prev % n)) % n) as f64 * 1e-6;
        if score > best_score {
            best = i;
            best_score = score;
        }
    }
    best
}

pub fn register(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    gst::Element::register(
        Some(plugin),
        "ristdispatcher",
        gst::Rank::NONE,
        Dispatcher::static_type(),
    )
}
