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
            7 => {
                let disp = value.get::<Option<gst::Element>>().ok().flatten();
                *self.inner.dispatcher.lock() = disp.clone();
                
                // Disable auto-balance on the dispatcher when dynbitrate is connected
                if let Some(ref dispatcher) = disp {
                    dispatcher.set_property("auto-balance", false);
                    gst::info!(CAT, "Connected to dispatcher and disabled auto-balance to prevent dueling controllers");
                } else {
                    gst::debug!(CAT, "Disconnected from dispatcher");
                }
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

        let rist = rist.unwrap();
        let encoder = encoder.unwrap();

        // Parse RIST stats and possibly drive dispatcher weights
        let stats_value: glib::Value = rist.property("stats");
        if let Ok(Some(structure)) = stats_value.get::<Option<gst::Structure>>() {
            gst::debug!(CAT, "Got RIST stats structure: {}", structure.to_string());
            
            // If we have a dispatcher, compute and set weights based on stats
            if let Some(ref disp) = dispatcher {
                self.update_dispatcher_weights(&structure, disp);
            }
            
            // Update bitrate based on aggregate stats
            self.update_bitrate_from_stats(&structure, &encoder);
        } else {
            // Fall back to simple adjustment if no stats available
            self.simple_bitrate_adjustment(&encoder);
        }
    }
    
    fn update_dispatcher_weights(&self, stats: &gst::Structure, dispatcher: &gst::Element) {
        // Extract per-session stats and compute EWMA weights
        let mut weights = Vec::new();
        let mut session_idx = 0;
        
        // Try to find session-based stats
        loop {
            let session_key = format!("session-{}", session_idx);
            
            // Check if this session exists in the stats
            if let Ok(sent_original) = stats.get::<u64>(&format!("{}.sent-original-packets", session_key))
                .or_else(|_| stats.get::<u64>("sent-original-packets")) {
                
                let sent_retrans = stats.get::<u64>(&format!("{}.sent-retransmitted-packets", session_key))
                    .or_else(|_| stats.get::<u64>("sent-retransmitted-packets"))
                    .unwrap_or(0);
                
                let rtt_ms = stats.get::<f64>(&format!("{}.round-trip-time", session_key))
                    .or_else(|_| stats.get::<f64>("round-trip-time"))
                    .unwrap_or(50.0);
                
                // Simple EWMA-based weight calculation
                let total_sent = sent_original + sent_retrans;
                let rtx_rate = if total_sent > 0 {
                    sent_retrans as f64 / total_sent as f64
                } else {
                    0.0
                };
                
                // Base weight from goodput (inverse of RTX rate and RTT)
                let mut weight = 1.0 / (1.0 + 0.1 * rtx_rate); // Penalty for retransmissions
                weight /= 1.0 + 0.01 * (rtt_ms / 100.0);       // Penalty for high RTT
                weight = weight.max(0.05);                       // Floor to prevent starvation
                
                weights.push(weight);
                session_idx += 1;
                
                gst::debug!(CAT, "Session {}: sent={}, rtx={}, rtt={:.1}ms, weight={:.3}", 
                          session_idx-1, sent_original, sent_retrans, rtt_ms, weight);
            } else {
                // No more sessions
                break;
            }
        }
        
        // If we found multi-session stats, normalize and set dispatcher weights
        if weights.len() > 1 {
            let total: f64 = weights.iter().sum();
            if total > 0.0 {
                for w in &mut weights {
                    *w /= total;
                }
            }
            
            let weights_json = serde_json::to_string(&weights).unwrap_or_default();
            dispatcher.set_property("weights", &weights_json);
            gst::info!(CAT, "Updated dispatcher weights: {}", weights_json);
        }
    }
    
    fn update_bitrate_from_stats(&self, stats: &gst::Structure, encoder: &gst::Element) {
        // Get aggregate loss rate and RTT
        let total_original = stats.get::<u64>("sent-original-packets").unwrap_or(0);
        let total_retrans = stats.get::<u64>("sent-retransmitted-packets").unwrap_or(0);
        let avg_rtt = stats.get::<f64>("round-trip-time").unwrap_or(50.0);
        
        if total_original == 0 {
            return; // No data yet
        }
        
        let total_sent = total_original + total_retrans;
        let loss_rate = total_retrans as f64 / total_sent as f64;
        
        let target_loss = *self.inner.target_loss_pct.lock() / 100.0;
        let rtt_threshold = *self.inner.rtt_floor_ms.lock() as f64;
        
        // Get current bitrate
        let current_kbps: u32 = encoder.property("bitrate");
        let min = *self.inner.min_kbps.lock();
        let max = *self.inner.max_kbps.lock();
        let step = *self.inner.step_kbps.lock();
        
        // Rate limiting
        let now = Instant::now();
        let last_change = *self.inner.last_change.lock();
        let since = last_change
            .map(|t| now.duration_since(t))
            .unwrap_or(Duration::from_secs(1));
            
        if since < Duration::from_millis(1200) {
            return; // Too soon to change
        }
        
        let mut new_kbps = current_kbps;
        
        // Adjust based on loss rate and RTT
        if loss_rate > target_loss || avg_rtt > rtt_threshold {
            // Decrease bitrate due to high loss or RTT
            new_kbps = current_kbps.saturating_sub(step).max(min);
            gst::info!(CAT, "Decreasing bitrate from {} to {} kbps (loss={:.2}%, rtt={:.1}ms)", 
                      current_kbps, new_kbps, loss_rate * 100.0, avg_rtt);
        } else if loss_rate < target_loss * 0.5 && avg_rtt < rtt_threshold * 0.8 {
            // Increase bitrate due to good conditions
            new_kbps = (current_kbps + step).min(max);
            gst::info!(CAT, "Increasing bitrate from {} to {} kbps (loss={:.2}%, rtt={:.1}ms)", 
                      current_kbps, new_kbps, loss_rate * 100.0, avg_rtt);
        }
        
        if new_kbps != current_kbps {
            encoder.set_property("bitrate", &new_kbps);
            *self.inner.last_change.lock() = Some(now);
        }
    }
    
    fn simple_bitrate_adjustment(&self, encoder: &gst::Element) {
        // Fallback to simple oscillation if no stats
        let current_kbps: u32 = encoder.property("bitrate");
        let min = *self.inner.min_kbps.lock();
        let max = *self.inner.max_kbps.lock();
        let step = *self.inner.step_kbps.lock();

        let now = Instant::now();
        let last_change = *self.inner.last_change.lock();
        let since = last_change
            .map(|t| now.duration_since(t))
            .unwrap_or(Duration::from_secs(1));

        if since < Duration::from_millis(1200) {
            return; // Too soon to change
        }

        let mut new_kbps = current_kbps;

        if current_kbps >= max {
            new_kbps = current_kbps.saturating_sub(step).max(min);
            gst::info!(CAT, "Decreasing bitrate from {} to {} kbps (demo mode)", current_kbps, new_kbps);
        } else if current_kbps <= min {
            new_kbps = (current_kbps + step).min(max);
            gst::info!(CAT, "Increasing bitrate from {} to {} kbps (demo mode)", current_kbps, new_kbps);
        }

        if new_kbps != current_kbps {
            encoder.set_property("bitrate", &new_kbps);
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
