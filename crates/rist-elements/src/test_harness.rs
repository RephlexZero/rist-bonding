//! Test harness elements for RIST dispatcher testing
//!
//! This module contains mock elements that are only compiled when the 'test-plugin' feature is enabled.
//! These elements provide controlled testing environments for the RIST dispatcher and dynamic bitrate controller.

use anyhow::Result;
use glib::subclass::prelude::*;
use gst::glib;
use gst::prelude::*;
use gst::subclass::prelude::{ElementImpl, GstObjectImpl};
use gstreamer as gst;
use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Register all test harness elements
pub fn register_test_elements() -> Result<()> {
    let _ = gst::init();

    counter_sink::register()?;
    encoder_stub::register()?;
    riststats_mock::register()?;

    Ok(())
}

// Re-export test elements
pub use riststats_mock::RistStatsMock;

/// Counter sink: counts buffers and records EOS/FLUSH events
/// Useful for verifying that the correct number of buffers flow through pipelines
pub mod counter_sink {
    use super::*;

    #[derive(Default)]
    pub struct Inner {
        count: AtomicU64,
        got_eos: AtomicU64,
        got_flush_start: AtomicU64,
        got_flush_stop: AtomicU64,
    }

    glib::wrapper! {
        pub struct CounterSink(ObjectSubclass<Impl>) @extends gst::Element, gst::Object;
    }

    #[derive(Default)]
    pub struct Impl {
        inner: Arc<Inner>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Impl {
        const NAME: &'static str = "counter_sink";
        type Type = CounterSink;
        type ParentType = gst::Element;
    }

    impl ObjectImpl for Impl {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            let sink_tmpl = gst::PadTemplate::new(
                "sink",
                gst::PadDirection::Sink,
                gst::PadPresence::Always,
                &gst::Caps::new_any(),
            )
            .unwrap();

            let inner = self.inner.clone();
            let sinkpad = gst::Pad::builder_from_template(&sink_tmpl)
                .name("sink")
                .chain_function(move |_pad, _parent, _buf| {
                    inner.count.fetch_add(1, Ordering::Relaxed);
                    Ok(gst::FlowSuccess::Ok)
                })
                .event_function({
                    let inner = self.inner.clone();
                    move |_pad, _parent, event| match event.type_() {
                        gst::EventType::Eos => {
                            inner.got_eos.store(1, Ordering::Relaxed);
                            true
                        }
                        gst::EventType::FlushStart => {
                            inner.got_flush_start.store(1, Ordering::Relaxed);
                            true
                        }
                        gst::EventType::FlushStop => {
                            inner.got_flush_stop.store(1, Ordering::Relaxed);
                            true
                        }
                        _ => gst::Pad::event_default(_pad, _parent, event),
                    }
                })
                .build();

            obj.add_pad(&sinkpad).unwrap();
        }

        fn properties() -> &'static [glib::ParamSpec] {
            static PROPS: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecUInt64::builder("count")
                        .flags(glib::ParamFlags::READABLE)
                        .build(),
                    glib::ParamSpecBoolean::builder("got-eos")
                        .flags(glib::ParamFlags::READABLE)
                        .build(),
                    glib::ParamSpecBoolean::builder("got-flush-start")
                        .flags(glib::ParamFlags::READABLE)
                        .build(),
                    glib::ParamSpecBoolean::builder("got-flush-stop")
                        .flags(glib::ParamFlags::READABLE)
                        .build(),
                ]
            });
            PROPS.as_ref()
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "count" => self.inner.count.load(Ordering::Relaxed).to_value(),
                "got-eos" => (self.inner.got_eos.load(Ordering::Relaxed) != 0).to_value(),
                "got-flush-start" => (self.inner.got_flush_start.load(Ordering::Relaxed) != 0).to_value(),
                "got-flush-stop" => (self.inner.got_flush_stop.load(Ordering::Relaxed) != 0).to_value(),
                _ => false.to_value(),
            }
        }
    }

    impl GstObjectImpl for Impl {}

    impl ElementImpl for Impl {
        fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
            static META: Lazy<gst::subclass::ElementMetadata> = Lazy::new(|| {
                gst::subclass::ElementMetadata::new(
                    "Counter Sink",
                    "Sink/Testing",
                    "Counts buffers and events for testing",
                    "RIST Test Harness",
                )
            });
            Some(&*META)
        }

        fn pad_templates() -> &'static [gst::PadTemplate] {
            static TEMPLS: Lazy<Vec<gst::PadTemplate>> = Lazy::new(|| {
                vec![gst::PadTemplate::new(
                    "sink",
                    gst::PadDirection::Sink,
                    gst::PadPresence::Always,
                    &gst::Caps::new_any(),
                )
                .unwrap()]
            });
            TEMPLS.as_ref()
        }
    }

    pub fn register() -> Result<(), glib::BoolError> {
        gst::Element::register(
            None,
            "counter_sink",
            gst::Rank::NONE,
            CounterSink::static_type(),
        )
    }
}

/// Encoder stub: passthrough with bitrate property and optional key unit signal
/// Simulates an encoder for testing dynamic bitrate adjustment
pub mod encoder_stub {
    use super::*;

    pub struct Inner {
        bitrate_kbps: Mutex<u32>,
    }

    impl Default for Inner {
        fn default() -> Self {
            Self {
                bitrate_kbps: Mutex::new(3000),
            }
        }
    }

    glib::wrapper! {
        pub struct EncoderStub(ObjectSubclass<Impl>) @extends gst::Element, gst::Object;
    }

    #[derive(Default)]
    pub struct Impl {
        inner: Arc<Inner>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Impl {
        const NAME: &'static str = "encoder_stub";
        type Type = EncoderStub;
        type ParentType = gst::Element;
    }

    impl ObjectImpl for Impl {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            let sink_tmpl = gst::PadTemplate::new(
                "sink",
                gst::PadDirection::Sink,
                gst::PadPresence::Always,
                &gst::Caps::new_any(),
            )
            .unwrap();
            let src_tmpl = gst::PadTemplate::new(
                "src",
                gst::PadDirection::Src,
                gst::PadPresence::Always,
                &gst::Caps::new_any(),
            )
            .unwrap();

            let srcpad = gst::Pad::builder_from_template(&src_tmpl)
                .name("src")
                .build();
            let sinkpad = gst::Pad::builder_from_template(&sink_tmpl)
                .name("sink")
                .chain_function(|_pad, parent, buffer| {
                    match parent.and_then(|p| p.downcast_ref::<EncoderStub>()) {
                        Some(elem) => match elem.static_pad("src") {
                            Some(src) => src.push(buffer),
                            None => Err(gst::FlowError::Error),
                        },
                        None => Err(gst::FlowError::Error),
                    }
                })
                .event_function(|pad, parent, event| {
                    if let Some(elem) = parent.and_then(|p| p.downcast_ref::<EncoderStub>()) {
                        if let Some(src) = elem.static_pad("src") {
                            return src.push_event(event);
                        }
                    }
                    gst::Pad::event_default(pad, parent, event)
                })
                .build();

            obj.add_pad(&sinkpad).unwrap();
            obj.add_pad(&srcpad).unwrap();
        }

        fn properties() -> &'static [glib::ParamSpec] {
            static PROPS: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecUInt::builder("bitrate")
                    .nick("Bitrate (kbps)")
                    .default_value(3000)
                    .minimum(100)
                    .maximum(100000)
                    .build()]
            });
            PROPS.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "bitrate" => {
                    let v = value.get::<u32>().unwrap_or(3000);
                    *self.inner.bitrate_kbps.lock().unwrap() = v;
                }
                _ => {}
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "bitrate" => {
                    let val = *self.inner.bitrate_kbps.lock().unwrap();
                    val.to_value()
                }
                _ => 0u32.to_value(),
            }
        }

        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGS: Lazy<Vec<glib::subclass::Signal>> =
                Lazy::new(|| vec![glib::subclass::Signal::builder("force-key-unit").build()]);
            SIGS.as_ref()
        }
    }

    impl GstObjectImpl for Impl {}

    impl ElementImpl for Impl {
        fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
            static META: Lazy<gst::subclass::ElementMetadata> = Lazy::new(|| {
                gst::subclass::ElementMetadata::new(
                    "Encoder Stub",
                    "Filter/Testing",
                    "Passthrough encoder with bitrate property for testing",
                    "RIST Test Harness",
                )
            });
            Some(&*META)
        }

        fn pad_templates() -> &'static [gst::PadTemplate] {
            static TEMPLS: Lazy<Vec<gst::PadTemplate>> = Lazy::new(|| {
                vec![
                    gst::PadTemplate::new(
                        "sink",
                        gst::PadDirection::Sink,
                        gst::PadPresence::Always,
                        &gst::Caps::new_any(),
                    )
                    .unwrap(),
                    gst::PadTemplate::new(
                        "src",
                        gst::PadDirection::Src,
                        gst::PadPresence::Always,
                        &gst::Caps::new_any(),
                    )
                    .unwrap(),
                ]
            });
            TEMPLS.as_ref()
        }
    }

    pub fn register() -> Result<(), glib::BoolError> {
        gst::Element::register(
            None,
            "encoder_stub",
            gst::Rank::NONE,
            EncoderStub::static_type(),
        )
    }
}

/// RIST stats mock: provides controllable mock statistics for testing
/// Exposes a `stats` property with session-stats array and helpers to mutate
pub mod riststats_mock {
    use super::*;

    static CAT: Lazy<gst::DebugCategory> = Lazy::new(|| {
        gst::DebugCategory::new(
            "riststats-mock",
            gst::DebugColorFlags::empty(),
            Some("Mock RIST statistics provider"),
        )
    });

    #[derive(Clone, Debug, Default)]
    struct SessionModel {
        sent_original: u64,
        sent_retrans: u64,
        rtt_ms: u64,
    }

    #[derive(Debug)]
    struct Model {
        sessions: Vec<SessionModel>,
        custom_stats: Option<gst::Structure>,
        quality: f64,
        rtt: u32,
    }

    impl Default for Model {
        fn default() -> Self {
            Self {
                sessions: vec![SessionModel::default(); 2],
                custom_stats: None,
                quality: 95.0,
                rtt: 10,
            }
        }
    }

    glib::wrapper! {
        pub struct RistStatsMock(ObjectSubclass<Impl>) @extends gst::Element, gst::Object;
    }

    #[derive(Default)]
    pub struct Impl {
        model: Arc<Mutex<Model>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Impl {
        const NAME: &'static str = "riststats_mock";
        type Type = RistStatsMock;
        type ParentType = gst::Element;
    }

    impl ObjectImpl for Impl {
        fn constructed(&self) {
            self.parent_constructed();
        }

        fn properties() -> &'static [glib::ParamSpec] {
            static PROPS: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecBoxed::builder::<gst::Structure>("stats")
                        .nick("Stats structure")
                        .flags(glib::ParamFlags::READABLE | glib::ParamFlags::WRITABLE)
                        .build(),
                    glib::ParamSpecDouble::builder("quality")
                        .nick("Quality percentage")
                        .blurb("Link quality as percentage (0.0-100.0)")
                        .minimum(0.0)
                        .maximum(100.0)
                        .default_value(95.0)
                        .flags(glib::ParamFlags::READABLE | glib::ParamFlags::WRITABLE)
                        .build(),
                    glib::ParamSpecUInt::builder("rtt")
                        .nick("Round-trip time")
                        .blurb("Round-trip time in milliseconds")
                        .minimum(0)
                        .maximum(10000)
                        .default_value(10)
                        .flags(glib::ParamFlags::READABLE | glib::ParamFlags::WRITABLE)
                        .build(),
                ]
            });
            PROPS.as_ref()
        }

        fn set_property(&self, id: usize, value: &glib::Value, _pspec: &glib::ParamSpec) {
            match id {
                1 => {
                    if let Ok(s) = value.get::<gst::Structure>() {
                        let mut model = self.model.lock().unwrap();
                        model.custom_stats = Some(s);
                    }
                }
                2 => {
                    if let Ok(quality) = value.get::<f64>() {
                        let mut model = self.model.lock().unwrap();
                        model.quality = quality;
                    }
                }
                3 => {
                    if let Ok(rtt) = value.get::<u32>() {
                        let mut model = self.model.lock().unwrap();
                        model.rtt = rtt;
                    }
                }
                _ => {}
            }
        }

        fn property(&self, id: usize, _pspec: &glib::ParamSpec) -> glib::Value {
            match id {
                1 => {
                    let model = self.model.lock().unwrap();
                    if let Some(ref custom) = model.custom_stats {
                        return custom.to_value();
                    }
                    drop(model);
                    let s = self.build_stats_structure();
                    s.to_value()
                }
                2 => {
                    let model = self.model.lock().unwrap();
                    model.quality.to_value()
                }
                3 => {
                    let model = self.model.lock().unwrap();
                    model.rtt.to_value()
                }
                _ => gst::Structure::builder("rist/x-sender-stats")
                    .build()
                    .to_value(),
            }
        }
    }

    impl Impl {
        fn build_stats_structure(&self) -> gst::Structure {
            let model = self.model.lock().unwrap();
            let mut builder = gst::Structure::builder("rist/x-sender-stats");

            // Aggregated totals for compatibility with parsers expecting global fields
            let mut total_original: u64 = 0;
            let mut total_retrans: u64 = 0;
            let mut min_rtt: f64 = f64::INFINITY;
            for (i, sess) in model.sessions.iter().enumerate() {
                let prefix = format!("session-{}.", i);
                builder = builder
                    .field(
                        &format!("{}sent-original-packets", prefix),
                        sess.sent_original,
                    )
                    .field(
                        &format!("{}sent-retransmitted-packets", prefix),
                        sess.sent_retrans,
                    )
                    .field(&format!("{}round-trip-time", prefix), sess.rtt_ms as f64);

                total_original = total_original.saturating_add(sess.sent_original);
                total_retrans = total_retrans.saturating_add(sess.sent_retrans);
                let rtt_f = sess.rtt_ms as f64;
                if rtt_f > 0.0 && rtt_f < min_rtt {
                    min_rtt = rtt_f;
                }
            }

            // Add aggregated fields used by dynbitrate's fallback parser
            builder = builder
                .field("sent-original-packets", total_original)
                .field("sent-retransmitted-packets", total_retrans)
                .field("round-trip-time", if min_rtt.is_finite() { min_rtt } else { 0.0 });
            builder.build()
        }
    }

    impl RistStatsMock {
        /// Set the number of mock sessions
        pub fn set_sessions(&self, n: usize) {
            let imp = self.imp();
            let mut model = imp.model.lock().unwrap();
            model.sessions = vec![SessionModel::default(); n];
            drop(model);
            self.notify("stats");
        }

        /// Simulate traffic progression
        pub fn tick(&self, delta_original: &[u64], delta_retrans: &[u64], rtt_ms: &[u64]) {
            let imp = self.imp();
            let mut model = imp.model.lock().unwrap();
            let n = model.sessions.len();
            for i in 0..n {
                if let Some(sess) = model.sessions.get_mut(i) {
                    sess.sent_original = sess
                        .sent_original
                        .saturating_add(delta_original.get(i).copied().unwrap_or(0));
                    sess.sent_retrans = sess
                        .sent_retrans
                        .saturating_add(delta_retrans.get(i).copied().unwrap_or(0));
                    sess.rtt_ms = rtt_ms.get(i).copied().unwrap_or(sess.rtt_ms);
                }
            }
            drop(model);
            self.notify("stats");
        }

        /// Simulate network degradation
        pub fn degrade(&self, idx: usize, extra_retrans: u64, new_rtt: u64) {
            let imp = self.imp();
            let mut model = imp.model.lock().unwrap();
            if let Some(sess) = model.sessions.get_mut(idx) {
                sess.sent_retrans = sess.sent_retrans.saturating_add(extra_retrans);
                sess.rtt_ms = new_rtt;
            }
            drop(model);
            self.notify("stats");
        }

        /// Simulate network recovery
        pub fn recover(&self, idx: usize) {
            let imp = self.imp();
            let mut model = imp.model.lock().unwrap();

            // Handle custom stats recovery
            if let Some(ref custom_stats) = model.custom_stats {
                let session_key = format!("session-{}", idx);

                let current_retrans = custom_stats
                    .get::<u64>(&format!("{}.sent-retransmitted-packets", session_key))
                    .unwrap_or(0);
                let current_rtt = custom_stats
                    .get::<f64>(&format!("{}.round-trip-time", session_key))
                    .unwrap_or(100.0);
                let current_original = custom_stats
                    .get::<u64>(&format!("{}.sent-original-packets", session_key))
                    .unwrap_or(1000);

                let recovery_factor = 0.3;
                let new_retrans = (current_retrans as f64 * recovery_factor) as u64;

                let target_rtt = 25.0;
                let new_rtt = if current_rtt > target_rtt {
                    let rtt_improvement = (current_rtt - target_rtt) * 0.6;
                    (current_rtt - rtt_improvement.max(1.0)).max(target_rtt)
                } else {
                    target_rtt
                };

                let new_original = current_original.saturating_add(100);

                let mut builder = gst::Structure::builder(custom_stats.name());

                for session_idx in 0..2 {
                    let sess_key = format!("session-{}", session_idx);
                    let orig_key = format!("{}.sent-original-packets", sess_key);
                    let retrans_key = format!("{}.sent-retransmitted-packets", sess_key);
                    let rtt_key = format!("{}.round-trip-time", sess_key);

                    if session_idx == idx {
                        builder = builder
                            .field(&orig_key, new_original)
                            .field(&retrans_key, new_retrans)
                            .field(&rtt_key, new_rtt);
                    } else {
                        if let Ok(orig_val) = custom_stats.get::<u64>(&orig_key) {
                            builder = builder.field(&orig_key, orig_val);
                        }
                        if let Ok(retrans_val) = custom_stats.get::<u64>(&retrans_key) {
                            builder = builder.field(&retrans_key, retrans_val);
                        }
                        if let Ok(rtt_val) = custom_stats.get::<f64>(&rtt_key) {
                            builder = builder.field(&rtt_key, rtt_val);
                        }
                    }
                }

                model.custom_stats = Some(builder.build());

                gst::debug!(
                    CAT,
                    "Session {} recovered: retrans={}→{}, rtt={:.1}ms→{:.1}ms, original={}→{}",
                    idx,
                    current_retrans,
                    new_retrans,
                    current_rtt,
                    new_rtt,
                    current_original,
                    new_original
                );
            }

            // Also update session model
            if let Some(sess) = model.sessions.get_mut(idx) {
                let recovery_factor = 0.3;
                sess.sent_retrans = (sess.sent_retrans as f64 * recovery_factor) as u64;

                let target_rtt = 25u64;
                let current_rtt = sess.rtt_ms;

                if current_rtt > target_rtt {
                    let rtt_improvement = ((current_rtt - target_rtt) as f64 * 0.6) as u64;
                    sess.rtt_ms = current_rtt - rtt_improvement.max(1);
                } else if current_rtt < target_rtt {
                    sess.rtt_ms = target_rtt;
                }

                let base_increment = 100u64;
                sess.sent_original = sess.sent_original.saturating_add(base_increment);
            }

            drop(model);
            self.notify("stats");
        }
    }

    impl GstObjectImpl for Impl {}

    impl ElementImpl for Impl {
        fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
            static ELEMENT_METADATA: Lazy<gst::subclass::ElementMetadata> = Lazy::new(|| {
                gst::subclass::ElementMetadata::new(
                    "RIST Stats Mock",
                    "Test/Source",
                    "Test element that provides mock RIST sender statistics",
                    "RIST Test Harness",
                )
            });
            Some(&*ELEMENT_METADATA)
        }
    }

    pub fn register() -> Result<(), glib::BoolError> {
        gst::Element::register(
            None,
            "riststats_mock",
            gst::Rank::NONE,
            RistStatsMock::static_type(),
        )
    }
}
