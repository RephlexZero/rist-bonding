use anyhow::Result;
use glib::subclass::prelude::*;
use gst::glib;
use gst::prelude::*;
use gst::subclass::prelude::{ElementImpl, GstObjectImpl};
use gstreamer as gst;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

// Re-export plugin test registration
pub fn register_everything_for_tests() {
    let _ = gst::init();
    // Static register the plugin under test
    ristsmart::register_for_tests();

    // Register harness elements
    counter_sink::register().expect("register counter_sink");
    encoder_stub::register().expect("register encoder_stub");
    riststats_mock::register().expect("register riststats_mock");
}

// 1) counter_sink: counts buffers and records EOS/FLUSH
mod counter_sink {
    use super::*;

    #[derive(Default)]
    pub struct Inner {
        count: AtomicU64,
        got_eos: AtomicU64,
        got_flush_start: AtomicU64,
        got_flush_stop: AtomicU64,
    }

    ::glib::wrapper! {
        pub struct CounterSink(ObjectSubclass<Impl>) @extends gst::Element, gst::Object;
    }

    #[derive(Default)]
    pub struct Impl {
        inner: Arc<Inner>,
    }

    #[::glib::object_subclass]
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
            use once_cell::sync::Lazy;
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

        fn property(&self, id: usize, _pspec: &glib::ParamSpec) -> glib::Value {
            match id {
                0 => self.inner.count.load(Ordering::Relaxed).to_value(),
                1 => (self.inner.got_eos.load(Ordering::Relaxed) != 0).to_value(),
                2 => (self.inner.got_flush_start.load(Ordering::Relaxed) != 0).to_value(),
                3 => (self.inner.got_flush_stop.load(Ordering::Relaxed) != 0).to_value(),
                _ => 0u32.to_value(),
            }
        }
    }

    impl GstObjectImpl for Impl {}

    impl ElementImpl for Impl {
        fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
            use once_cell::sync::Lazy;
            static META: Lazy<gst::subclass::ElementMetadata> = Lazy::new(|| {
                gst::subclass::ElementMetadata::new(
                    "Counter Sink",
                    "Sink/Testing",
                    "Counts buffers and events for testing",
                    "tests",
                )
            });
            Some(&*META)
        }

        fn pad_templates() -> &'static [gst::PadTemplate] {
            use once_cell::sync::Lazy;
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

// 2) encoder_stub: passthrough with bitrate property and optional key unit signal
mod encoder_stub {
    use super::*;

    #[derive(Default)]
    pub struct Inner {
        bitrate_kbps: Mutex<u32>,
    }

    ::glib::wrapper! {
        pub struct EncoderStub(ObjectSubclass<Impl>) @extends gst::Element, gst::Object;
    }

    #[derive(Default)]
    pub struct Impl {
        inner: Arc<Inner>,
    }

    #[::glib::object_subclass]
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
                    // Passthrough events downstream
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
            use once_cell::sync::Lazy;
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

        fn set_property(&self, id: usize, value: &glib::Value, _pspec: &glib::ParamSpec) {
            if id == 0 {
                let v = value.get::<u32>().unwrap_or(3000);
                *self.inner.bitrate_kbps.lock().unwrap() = v;
            }
        }

        fn property(&self, id: usize, _pspec: &glib::ParamSpec) -> glib::Value {
            if id == 0 {
                return (*self.inner.bitrate_kbps.lock().unwrap()).to_value();
            }
            0u32.to_value()
        }

        fn signals() -> &'static [glib::subclass::Signal] {
            use once_cell::sync::Lazy;
            static SIGS: Lazy<Vec<glib::subclass::Signal>> =
                Lazy::new(|| vec![glib::subclass::Signal::builder("force-key-unit").build()]);
            SIGS.as_ref()
        }
    }

    impl GstObjectImpl for Impl {}

    impl ElementImpl for Impl {
        fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
            use once_cell::sync::Lazy;
            static META: Lazy<gst::subclass::ElementMetadata> = Lazy::new(|| {
                gst::subclass::ElementMetadata::new(
                    "Encoder Stub",
                    "Filter/Testing",
                    "Passthrough encoder with bitrate property",
                    "tests",
                )
            });
            Some(&*META)
        }

        fn pad_templates() -> &'static [gst::PadTemplate] {
            use once_cell::sync::Lazy;
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

// 3) riststats_mock: exposes a `stats` property with session-stats array and helpers to mutate
mod riststats_mock {
    use super::*;
    use once_cell::sync::Lazy;

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
        custom_stats: Option<gst::Structure>, // Store custom stats structure
    }

    impl Default for Model {
        fn default() -> Self {
            Self {
                sessions: vec![SessionModel::default(); 2], // Default to 2 sessions for testing
                custom_stats: None,
            }
        }
    }

    ::glib::wrapper! {
        pub struct RistStatsMock(ObjectSubclass<Impl>) @extends gst::Element, gst::Object;
    }

    #[derive(Default)]
    pub struct Impl {
        model: Arc<Mutex<Model>>,
    }

    #[::glib::object_subclass]
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
            use once_cell::sync::Lazy;
            static PROPS: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecBoxed::builder::<gst::Structure>("stats")
                    .nick("Stats structure")
                    .flags(glib::ParamFlags::READABLE | glib::ParamFlags::WRITABLE)
                    .build()]
            });
            PROPS.as_ref()
        }

        fn set_property(&self, id: usize, value: &glib::Value, _pspec: &glib::ParamSpec) {
            if id == 1 {
                // GStreamer uses 1-based indexing
                // Store custom stats structure for testing
                if let Ok(s) = value.get::<gst::Structure>() {
                    let mut model = self.model.lock().unwrap();
                    model.custom_stats = Some(s);
                }
            }
        }

        fn property(&self, id: usize, _pspec: &glib::ParamSpec) -> glib::Value {
            if id == 1 {
                // GStreamer uses 1-based indexing
                // Return custom stats if available, otherwise build default structure
                let model = self.model.lock().unwrap();
                if let Some(ref custom) = model.custom_stats {
                    return custom.to_value();
                }
                drop(model);
                let s = self.build_stats_structure();
                return s.to_value();
            }
            gst::Structure::builder("rist/x-sender-stats")
                .build()
                .to_value()
        }
    }

    impl Impl {
        fn build_stats_structure(&self) -> gst::Structure {
            let model = self.model.lock().unwrap();
            let mut builder = gst::Structure::builder("rist/x-sender-stats");
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
            }
            builder.build()
        }
    }

    impl RistStatsMock {
        pub fn set_sessions(&self, n: usize) {
            let imp = self.imp();
            let mut model = imp.model.lock().unwrap();
            model.sessions = vec![SessionModel::default(); n];
            drop(model);
            self.notify("stats");
        }

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

        pub fn recover(&self, idx: usize) {
            let imp = self.imp();
            let mut model = imp.model.lock().unwrap();
            if let Some(sess) = model.sessions.get_mut(idx) {
                // No-op placeholder for now
                let _ = sess;
            }
            drop(model);
            self.notify("stats");
        }
    }

    impl GstObjectImpl for Impl {}
    impl ElementImpl for Impl {
        fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
            use once_cell::sync::Lazy;
            static ELEMENT_METADATA: Lazy<gst::subclass::ElementMetadata> = Lazy::new(|| {
                gst::subclass::ElementMetadata::new(
                    "RIST Stats Mock",
                    "Test/Source",
                    "Test element that provides mock RIST sender statistics",
                    "Test Harness",
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
