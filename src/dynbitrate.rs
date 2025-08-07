use gst::glib;
use gst::prelude::*;
use gst::subclass::prelude::*;
use gstreamer as gst;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::{Duration, Instant};

// Element: dynbitrate
// Bin with two mandatory child properties:
//   - property "encoder" (Element) : upstream encoder whose "bitrate" we set (kbps)
//   - property "rist"    (Element) : the ristsink we read stats from
// It also has optional properties:
//   - min-kbps, max-kbps, step-kbps
//   - target-loss-pct (NACK/loss target), min-rtx-rtt-ms
//   - downscale-keyunit (bool) – force keyframe on downscale

#[derive(Default)]
pub struct ControllerInner {
    encoder: Mutex<Option<gst::Element>>, // e.g. x265enc
    rist: Mutex<Option<gst::Element>>,    // the ristsink
    min_kbps: Mutex<u32>,
    max_kbps: Mutex<u32>,
    step_kbps: Mutex<u32>,
    target_loss_pct: Mutex<f64>,
    rtt_floor_ms: Mutex<u64>,
    last_change: Mutex<Option<Instant>>,
}

glib::wrapper! {
    pub struct DynBitrate(ObjectSubclass<ControllerImpl>) @extends gst::Element, gst::Object;
}

#[derive(Default)]
pub struct ControllerImpl {
    inner: Arc<ControllerInner>,
}

#[glib::object_subclass]
impl ObjectSubclass for ControllerImpl {
    const NAME: &'static str = "dynbitrate";
    type Type = DynBitrate;
    type ParentType = gst::Element; // It behaves like a purely-control element
}

impl ObjectImpl for ControllerImpl {
    fn constructed(&self) {
        self.parent_constructed();

        // Install a periodic tick to poll ristsink stats and adjust bitrate
        let obj = self.obj();
        let weak = obj.downgrade();
        gst::glib::timeout_add_local(Duration::from_millis(500), move || {
            if let Some(obj) = weak.upgrade() {
                obj.imp().tick();
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        });
    }

    fn properties() -> &'static [glib::ParamSpec] {
        use once_cell::sync::Lazy;
        static PROPS: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
            vec![
                glib::ParamSpecObject::builder::<gst::Element>("encoder").build(),
                glib::ParamSpecObject::builder::<gst::Element>("rist").build(),
                glib::ParamSpecUInt::builder("min-kbps")
                    .default_value(500)
                    .build(),
                glib::ParamSpecUInt::builder("max-kbps")
                    .default_value(8000)
                    .build(),
                glib::ParamSpecUInt::builder("step-kbps")
                    .default_value(250)
                    .build(),
                glib::ParamSpecDouble::builder("target-loss-pct")
                    .default_value(0.5)
                    .build(),
                glib::ParamSpecUInt64::builder("min-rtx-rtt-ms")
                    .default_value(40)
                    .build(),
                glib::ParamSpecBoolean::builder("downscale-keyunit")
                    .default_value(true)
                    .build(),
            ]
        });
        PROPS.as_ref()
    }

    fn set_property(&self, id: usize, value: &glib::Value, _pspec: &glib::ParamSpec) {
        match id {
            0 => *self.inner.encoder.lock() = value.get::<Option<gst::Element>>().ok().flatten(),
            1 => *self.inner.rist.lock() = value.get::<Option<gst::Element>>().ok().flatten(),
            2 => *self.inner.min_kbps.lock() = value.get::<u32>().unwrap_or(500),
            3 => *self.inner.max_kbps.lock() = value.get::<u32>().unwrap_or(8000),
            4 => *self.inner.step_kbps.lock() = value.get::<u32>().unwrap_or(250),
            5 => *self.inner.target_loss_pct.lock() = value.get::<f64>().unwrap_or(0.5),
            6 => *self.inner.rtt_floor_ms.lock() = value.get::<u64>().unwrap_or(40),
            7 => {
                let _ = value.get::<bool>(); /* stored via property API if needed */
            }
            _ => {}
        }
    }

    fn property(&self, id: usize, _pspec: &glib::ParamSpec) -> glib::Value {
        match id {
            0 => self.inner.encoder.lock().to_value(),
            1 => self.inner.rist.lock().to_value(),
            2 => self.inner.min_kbps.lock().to_value(),
            3 => self.inner.max_kbps.lock().to_value(),
            4 => self.inner.step_kbps.lock().to_value(),
            5 => self.inner.target_loss_pct.lock().to_value(),
            6 => self.inner.rtt_floor_ms.lock().to_value(),
            7 => true.to_value(), // downscale-keyunit default
            _ => unimplemented!(),
        }
    }
}

impl GstObjectImpl for ControllerImpl {}

impl ElementImpl for ControllerImpl {
    fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
        use once_cell::sync::Lazy;
        static ELEMENT_METADATA: Lazy<gst::subclass::ElementMetadata> = Lazy::new(|| {
            gst::subclass::ElementMetadata::new(
                "Dynamic Bitrate Controller",
                "Filter/Network",
                "Dynamic bitrate controller for encoder adjustment based on RIST stats",
                "Jake",
            )
        });
        Some(&*ELEMENT_METADATA)
    }
}

impl ControllerImpl {
    fn tick(&self) {
        // Read ristsink "stats" property -> GstStructure "rist/x-sender-stats"
        let rist = { self.inner.rist.lock().clone() };
        let encoder = { self.inner.encoder.lock().clone() };
        if rist.is_none() || encoder.is_none() {
            return;
        }
        let _rist = rist.unwrap();
        let encoder = encoder.unwrap();

        // For now, just implement a basic placeholder
        // TODO: Implement proper RIST stats parsing when ristsink is available
        let min = *self.inner.min_kbps.lock();
        let max = *self.inner.max_kbps.lock();
        let step = *self.inner.step_kbps.lock();

        let current_kbps: u32 = encoder.property("bitrate");
        let mut kbps = current_kbps;
        let now = Instant::now();
        let last_change = *self.inner.last_change.lock();
        let since = last_change
            .map(|t| now.duration_since(t))
            .unwrap_or(Duration::from_secs(1));
        let can_change = since > Duration::from_millis(800);

        if can_change {
            // Simple oscillation for demonstration
            // In production, this would read RIST stats to make decisions
            if kbps >= max {
                kbps = kbps.saturating_sub(step).max(min);
            } else if kbps <= min {
                kbps = (kbps + step).min(max);
            }
            encoder.set_property("bitrate", &kbps);
            *self.inner.last_change.lock() = Some(now);
        }
    }
}

pub fn register(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    gst::Element::register(
        Some(plugin),
        "dynbitrate",
        gst::Rank::NONE,
        DynBitrate::static_type(),
    )
}
