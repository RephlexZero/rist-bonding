use gst::glib;
use gst::prelude::*;
use gst::subclass::prelude::*;
use gstreamer as gst;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::{Duration, Instant};

// Logging category
use once_cell::sync::Lazy;
static CAT: Lazy<gst::DebugCategory> = Lazy::new(|| {
    gst::DebugCategory::new(
        "dynbitrate",
        gst::DebugColorFlags::empty(),
        Some("Dynamic Bitrate Controller"),
    )
});

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
    dispatcher: Mutex<Option<gst::Element>>, // the rist dispatcher for coordination
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
        let obj = self.obj();

        // Create simple passthrough pads
        let sink_template = gst::PadTemplate::new(
            "sink",
            gst::PadDirection::Sink,
            gst::PadPresence::Always,
            &gst::Caps::new_any(),
        )
        .unwrap();

        let src_template = gst::PadTemplate::new(
            "src",
            gst::PadDirection::Src,
            gst::PadPresence::Always,
            &gst::Caps::new_any(),
        )
        .unwrap();

        let sinkpad = gst::Pad::builder_from_template(&sink_template)
            .name("sink")
            .chain_function(|_pad, parent, buffer| {
                // Simple passthrough - forward to src pad
                match parent.and_then(|p| p.downcast_ref::<DynBitrate>()) {
                    Some(element) => match element.static_pad("src") {
                        Some(srcpad) => {
                            gst::trace!(CAT, "Forwarding buffer through dynbitrate");
                            srcpad.push(buffer)
                        }
                        None => {
                            gst::error!(CAT, "No source pad found");
                            Err(gst::FlowError::Error)
                        }
                    },
                    None => {
                        gst::error!(CAT, "Failed to get element reference");
                        Err(gst::FlowError::Error)
                    }
                }
            })
            .build();

        let srcpad = gst::Pad::builder_from_template(&src_template)
            .name("src")
            .build();

        obj.add_pad(&sinkpad).unwrap();
        obj.add_pad(&srcpad).unwrap();

        // Install a periodic tick to poll ristsink stats and adjust bitrate
        // Use the same interval as dispatcher to avoid conflicts
        let weak = obj.downgrade();
        gst::glib::timeout_add_local(Duration::from_millis(750), move || { // Offset slightly from dispatcher
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
                glib::ParamSpecObject::builder::<gst::Element>("encoder")
                    .nick("Encoder element")
                    .blurb("The encoder element to control bitrate for")
                    .build(),
                glib::ParamSpecObject::builder::<gst::Element>("rist")
                    .nick("RIST element")
                    .blurb("The RIST sink element to read statistics from")
                    .build(),
                glib::ParamSpecUInt::builder("min-kbps")
                    .nick("Minimum bitrate (kbps)")
                    .blurb("Minimum allowed bitrate in kilobits per second")
                    .minimum(100)
                    .maximum(100000)
                    .default_value(500)
                    .build(),
                glib::ParamSpecUInt::builder("max-kbps")
                    .nick("Maximum bitrate (kbps)")
                    .blurb("Maximum allowed bitrate in kilobits per second")
                    .minimum(500)
                    .maximum(100000)
                    .default_value(8000)
                    .build(),
                glib::ParamSpecUInt::builder("step-kbps")
                    .nick("Step size (kbps)")
                    .blurb("Bitrate adjustment step size in kilobits per second")
                    .minimum(50)
                    .maximum(5000)
                    .default_value(250)
                    .build(),
                glib::ParamSpecDouble::builder("target-loss-pct")
                    .nick("Target loss percentage")
                    .blurb("Target packet loss percentage for bitrate adjustment")
                    .minimum(0.0)
                    .maximum(10.0)
                    .default_value(0.5)
                    .build(),
                glib::ParamSpecUInt64::builder("min-rtx-rtt-ms")
                    .nick("Minimum RTX RTT (ms)")
                    .blurb("Minimum retransmission round-trip time in milliseconds")
                    .minimum(10)
                    .maximum(1000)
                    .default_value(40)
                    .build(),
                glib::ParamSpecObject::builder::<gst::Element>("dispatcher")
                    .nick("Dispatcher element")
                    .blurb("The RIST dispatcher element to coordinate with for unified control")
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
            7 => *self.inner.dispatcher.lock() = value.get::<Option<gst::Element>>().ok().flatten(),
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
            7 => self.inner.dispatcher.lock().to_value(),
            _ => {
                // Return a safe default value for unknown properties
                "".to_value()
            }
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

    fn pad_templates() -> &'static [gst::PadTemplate] {
        use once_cell::sync::Lazy;
        static PAD_TEMPLATES: Lazy<Vec<gst::PadTemplate>> = Lazy::new(|| {
            let caps = gst::Caps::new_any();

            let sink_pad_template = gst::PadTemplate::new(
                "sink",
                gst::PadDirection::Sink,
                gst::PadPresence::Always,
                &caps,
            )
            .unwrap();

            let src_pad_template = gst::PadTemplate::new(
                "src",
                gst::PadDirection::Src,
                gst::PadPresence::Always,
                &caps,
            )
            .unwrap();

            vec![sink_pad_template, src_pad_template]
        });
        PAD_TEMPLATES.as_ref()
    }
}

impl ControllerImpl {
    fn tick(&self) {
        // Read ristsink "stats" property -> GstStructure "rist/x-sender-stats"
        let rist = { self.inner.rist.lock().clone() };
        let encoder = { self.inner.encoder.lock().clone() };
        let dispatcher = { self.inner.dispatcher.lock().clone() };

        if rist.is_none() {
            gst::trace!(CAT, "No RIST element configured, skipping adjustment");
            return;
        }

        if encoder.is_none() {
            gst::trace!(CAT, "No encoder element configured, skipping adjustment");
            return;
        }

        let _rist = rist.unwrap();
        let encoder = encoder.unwrap();

        // If we have a dispatcher, coordinate with its rebalancing
        let should_adjust = if let Some(ref disp) = dispatcher {
            // Check if dispatcher is currently stable (not rebalancing)
            let disp_weights: glib::Value = disp.property("current-weights");
            if let Ok(Some(weights_str)) = disp_weights.get::<Option<String>>() {
                if let Ok(weights) = serde_json::from_str::<Vec<f64>>(&weights_str) {
                    // Only adjust if weights seem stable (not changing rapidly)
                    let weight_variance: f64 = weights.iter().map(|w| (w - 1.0 / weights.len() as f64).powi(2)).sum::<f64>() / weights.len() as f64;
                    weight_variance < 0.1 // Low variance = stable weights
                } else {
                    true // If we can't parse, proceed with adjustment
                }
            } else {
                true // No weights available, proceed
            }
        } else {
            true // No dispatcher, proceed normally
        };

        if !should_adjust {
            gst::trace!(CAT, "Dispatcher is rebalancing, skipping bitrate adjustment");
            return;
        }

        // Get current bitrate
        let current_kbps: u32 = encoder.property("bitrate");

        let min = *self.inner.min_kbps.lock();
        let max = *self.inner.max_kbps.lock();
        let step = *self.inner.step_kbps.lock();

        // Simple time-based adjustment for demonstration
        let now = Instant::now();
        let last_change = *self.inner.last_change.lock();
        let since = last_change
            .map(|t| now.duration_since(t))
            .unwrap_or(Duration::from_secs(1));

        if since < Duration::from_millis(1200) { // Longer delay when coordinating with dispatcher
            return; // Too soon to change
        }

        let mut new_kbps = current_kbps;

        // TODO: Implement proper RIST stats parsing when ristsink is available
        // For now, just implement a basic oscillation for demonstration
        if current_kbps >= max {
            new_kbps = current_kbps.saturating_sub(step).max(min);
            gst::info!(
                CAT,
                "Decreasing bitrate from {} to {} kbps",
                current_kbps,
                new_kbps
            );
        } else if current_kbps <= min {
            new_kbps = (current_kbps + step).min(max);
            gst::info!(
                CAT,
                "Increasing bitrate from {} to {} kbps",
                current_kbps,
                new_kbps
            );
        }

        if new_kbps != current_kbps {
            encoder.set_property("bitrate", &new_kbps);
            *self.inner.last_change.lock() = Some(now);
            
            // Notify dispatcher if available that we changed bitrate
            if let Some(ref disp) = dispatcher {
                gst::debug!(CAT, "Notified dispatcher of bitrate change to {} kbps", new_kbps);
                let structure = gst::Structure::builder("bitrate-changed")
                    .field("new-bitrate", &new_kbps)
                    .build();
                let message = gst::message::Application::new(structure);
                let _ = disp.post_message(message);
            }
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
