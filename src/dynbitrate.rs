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

pub struct ControllerInner {
    encoder: Mutex<Option<gst::Element>>,    // e.g. x265enc
    rist: Mutex<Option<gst::Element>>,       // the ristsink
    dispatcher: Mutex<Option<gst::Element>>, // the rist dispatcher for coordination
    min_kbps: Mutex<u32>,
    max_kbps: Mutex<u32>,
    step_kbps: Mutex<u32>,
    target_loss_pct: Mutex<f64>,
    rtt_floor_ms: Mutex<u64>,
    downscale_keyunit: Mutex<bool>, // Force keyframe on bitrate downscale
    last_change: Mutex<Option<Instant>>,
    // Encoder property detection cache
    bitrate_property: Mutex<Option<(String, f64)>>, // (property_name, scale_factor)
}

impl Default for ControllerInner {
    fn default() -> Self {
        Self {
            encoder: Mutex::new(None),
            rist: Mutex::new(None),
            dispatcher: Mutex::new(None),
            min_kbps: Mutex::new(1000),
            max_kbps: Mutex::new(10000),
            step_kbps: Mutex::new(100),
            target_loss_pct: Mutex::new(1.0),
            rtt_floor_ms: Mutex::new(10),
            downscale_keyunit: Mutex::new(false),
            last_change: Mutex::new(None),
            bitrate_property: Mutex::new(None),
        }
    }
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
        gst::glib::timeout_add_local(Duration::from_millis(750), move || {
            // Offset slightly from dispatcher
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
                glib::ParamSpecBoolean::builder("downscale-keyunit")
                    .nick("Force keyframe on downscale")
                    .blurb("Force a keyframe when bitrate is reduced significantly")
                    .default_value(false)
                    .build(),
            ]
        });
        PROPS.as_ref()
    }

    fn set_property(&self, id: usize, value: &glib::Value, _pspec: &glib::ParamSpec) {
        match id {
            0 => {
                let encoder = value.get::<Option<gst::Element>>().ok().flatten();
                if let Some(ref enc) = encoder {
                    // Detect and cache encoder bitrate property
                    self.detect_encoder_bitrate_property(enc);
                }
                *self.inner.encoder.lock() = encoder;
            }
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
            8 => {
                let downscale_keyunit = value.get::<bool>().unwrap_or(false);
                *self.inner.downscale_keyunit.lock() = downscale_keyunit;
                gst::debug!(CAT, "Set downscale-keyunit: {}", downscale_keyunit);
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
            8 => self.inner.downscale_keyunit.lock().to_value(),
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
    fn detect_encoder_bitrate_property(&self, encoder: &gst::Element) {
        // Try common bitrate property names and detect units
        let property_candidates = [
            ("bitrate", 1.0),           // x264enc, x265enc (kbps)
            ("target-bitrate", 1000.0), // some HW encoders (bps)
            ("target_bitrate", 1000.0), // alternative naming (bps)
            ("avg-bitrate", 1.0),       // some encoders (kbps)
            ("avg_bitrate", 1.0),       // alternative naming (kbps)
        ];

        let mut detected_property: Option<(String, f64)> = None;

        for (prop_name, scale_factor) in &property_candidates {
            if let Some(prop_spec) = encoder.find_property(prop_name) {
                // Check if it's a writable integer/uint property
                let flags = prop_spec.flags();
                if flags.contains(glib::ParamFlags::WRITABLE)
                    && !flags.contains(glib::ParamFlags::CONSTRUCT_ONLY)
                {
                    // Check the type
                    if prop_spec.value_type() == u32::static_type()
                        || prop_spec.value_type() == i32::static_type()
                    {
                        detected_property = Some((prop_name.to_string(), *scale_factor));
                        gst::info!(
                            CAT,
                            "Detected encoder bitrate property: '{}' with scale factor {}",
                            prop_name,
                            scale_factor
                        );
                        break;
                    }
                }
            }
        }

        if detected_property.is_none() {
            gst::warning!(
                CAT,
                "Could not detect encoder bitrate property, will try default 'bitrate'"
            );
            detected_property = Some(("bitrate".to_string(), 1.0));
        }

        *self.inner.bitrate_property.lock() = detected_property;
    }

    fn set_encoder_bitrate(
        &self,
        encoder: &gst::Element,
        kbps: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let current_kbps = self.get_encoder_bitrate(encoder);
        let bitrate_prop = self.inner.bitrate_property.lock().clone();

        let (prop_name, scale_factor) = bitrate_prop.unwrap_or_else(|| {
            gst::warning!(CAT, "No bitrate property detected, using default");
            ("bitrate".to_string(), 1.0)
        });

        // Convert kbps to the encoder's expected units
        let encoder_value = if scale_factor > 1.0 {
            // Convert kbps to bps
            (kbps as f64 * scale_factor) as u32
        } else {
            // Already in kbps
            kbps
        };

        gst::debug!(
            CAT,
            "Setting encoder property '{}' to {} (from {} kbps with scale {})",
            prop_name,
            encoder_value,
            kbps,
            scale_factor
        );

        encoder.set_property(&prop_name, encoder_value);

        // Force keyframe on significant downscale if enabled
        let downscale_keyunit = *self.inner.downscale_keyunit.lock();
        if downscale_keyunit && kbps < current_kbps {
            let downscale_ratio = current_kbps as f64 / kbps as f64;
            if downscale_ratio >= 1.5 {
                // Force keyframe for significant downscale (≥50% reduction)
                self.force_keyframe(encoder);
                gst::info!(
                    CAT,
                    "Forced keyframe due to significant bitrate downscale: {} -> {} kbps ({}% reduction)",
                    current_kbps,
                    kbps,
                    ((downscale_ratio - 1.0) * 100.0) as u32
                );
            }
        }

        Ok(())
    }

    fn force_keyframe(&self, encoder: &gst::Element) {
        // Try to force a keyframe using the force-key-unit event
        let structure = gst::Structure::builder("GstForceKeyUnit")
            .field("all-headers", true)
            .field("count", 1u32)
            .build();

        let event = gst::event::CustomDownstream::new(structure);

        // Try to send the event to the encoder's sink pad
        if let Some(sink_pad) = encoder.static_pad("sink") {
            if sink_pad.send_event(event) {
                gst::debug!(CAT, "Successfully sent force-key-unit event to encoder");
            } else {
                gst::warning!(
                    CAT,
                    "Failed to send force-key-unit event to encoder sink pad"
                );
                // Fallback: try sending to encoder element directly
                let fallback_event = gst::event::CustomDownstream::new(
                    gst::Structure::builder("GstForceKeyUnit")
                        .field("all-headers", true)
                        .field("count", 1u32)
                        .build(),
                );
                if !encoder.send_event(fallback_event) {
                    gst::warning!(
                        CAT,
                        "Failed to send force-key-unit event to encoder element"
                    );
                }
            }
        } else {
            gst::warning!(
                CAT,
                "Could not find sink pad on encoder for force-key-unit event"
            );
        }
    }

    fn get_encoder_bitrate(&self, encoder: &gst::Element) -> u32 {
        let bitrate_prop = self.inner.bitrate_property.lock().clone();

        let (prop_name, scale_factor) = bitrate_prop.unwrap_or_else(|| {
            gst::warning!(CAT, "No bitrate property detected, using default");
            ("bitrate".to_string(), 1.0)
        });

        let encoder_value: u32 = encoder.property(&prop_name);

        // Convert to kbps if needed
        if scale_factor > 1.0 {
            // Convert from bps to kbps
            (encoder_value as f64 / scale_factor) as u32
        } else {
            // Already in kbps
            encoder_value
        }
    }

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

        // Try parsing session-stats array first (correct format)
        if let Ok(sess_stats_value) = stats.get::<glib::Value>("session-stats") {
            if let Ok(sess_array) = sess_stats_value.get::<glib::ValueArray>() {
                for (session_idx, session_value) in sess_array.iter().enumerate() {
                    if let Ok(session_struct) = session_value.get::<gst::Structure>() {
                        let sent_original = session_struct
                            .get::<u64>("sent-original-packets")
                            .unwrap_or(0);
                        let sent_retrans = session_struct
                            .get::<u64>("sent-retransmitted-packets")
                            .unwrap_or(0);
                        let rtt_ms =
                            session_struct.get::<u64>("round-trip-time").unwrap_or(50) as f64;

                        // Simple EWMA-based weight calculation
                        let total_sent = sent_original + sent_retrans;
                        let rtx_rate = if total_sent > 0 {
                            sent_retrans as f64 / total_sent as f64
                        } else {
                            0.0
                        };

                        // Base weight from goodput (inverse of RTX rate and RTT)
                        let mut weight = 1.0 / (1.0 + 0.1 * rtx_rate); // Penalty for retransmissions
                        weight /= 1.0 + 0.01 * (rtt_ms / 100.0); // Penalty for high RTT
                        weight = weight.max(0.05); // Floor to prevent starvation

                        weights.push(weight);

                        gst::debug!(
                            CAT,
                            "Session {}: sent={}, rtx={}, rtt={:.1}ms, weight={:.3}",
                            session_idx,
                            sent_original,
                            sent_retrans,
                            rtt_ms,
                            weight
                        );
                    }
                }
            }
        }

        // Fallback to legacy parsing if session-stats array not found
        if weights.is_empty() {
            let mut session_idx = 0;

            // Try to find session-based stats using legacy format
            loop {
                let session_key = format!("session-{}", session_idx);

                // Check if this session exists in the stats
                if let Ok(sent_original) = stats
                    .get::<u64>(&format!("{}.sent-original-packets", session_key))
                    .or_else(|_| stats.get::<u64>("sent-original-packets"))
                {
                    let sent_retrans = stats
                        .get::<u64>(&format!("{}.sent-retransmitted-packets", session_key))
                        .or_else(|_| stats.get::<u64>("sent-retransmitted-packets"))
                        .unwrap_or(0);

                    let rtt_ms = stats
                        .get::<f64>(&format!("{}.round-trip-time", session_key))
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
                    weight /= 1.0 + 0.01 * (rtt_ms / 100.0); // Penalty for high RTT
                    weight = weight.max(0.05); // Floor to prevent starvation

                    weights.push(weight);
                    session_idx += 1;

                    gst::debug!(
                        CAT,
                        "Session {}: sent={}, rtx={}, rtt={:.1}ms, weight={:.3}",
                        session_idx - 1,
                        sent_original,
                        sent_retrans,
                        rtt_ms,
                        weight
                    );
                } else {
                    // No more sessions
                    break;
                }
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
        // Parse session-stats array to derive aggregate RTT and loss
        let mut total_original = 0u64;
        let mut total_retrans = 0u64;
        let mut rtts = Vec::new();

        // Try parsing session-stats array first (correct format)
        if let Ok(sess_stats_value) = stats.get::<glib::Value>("session-stats") {
            if let Ok(sess_array) = sess_stats_value.get::<glib::ValueArray>() {
                for session_value in sess_array.iter() {
                    if let Ok(session_struct) = session_value.get::<gst::Structure>() {
                        let sent_original = session_struct
                            .get::<u64>("sent-original-packets")
                            .unwrap_or(0);
                        let sent_retrans = session_struct
                            .get::<u64>("sent-retransmitted-packets")
                            .unwrap_or(0);
                        let rtt_ms =
                            session_struct.get::<u64>("round-trip-time").unwrap_or(50) as f64;

                        total_original += sent_original;
                        total_retrans += sent_retrans;
                        if rtt_ms > 0.0 {
                            rtts.push(rtt_ms);
                        }
                    }
                }
            }
        }

        // Fallback to legacy aggregated stats if session-stats not available
        if total_original == 0 && rtts.is_empty() {
            total_original = stats.get::<u64>("sent-original-packets").unwrap_or(0);
            total_retrans = stats.get::<u64>("sent-retransmitted-packets").unwrap_or(0);
            if let Ok(rtt) = stats.get::<f64>("round-trip-time") {
                if rtt > 0.0 {
                    rtts.push(rtt);
                }
            }
        }

        if total_original == 0 {
            return; // No data yet
        }

        let total_sent = total_original + total_retrans;
        let loss_rate = total_retrans as f64 / total_sent as f64;

        // Calculate aggregate RTT (min RTT for conservative estimate)
        let avg_rtt = if !rtts.is_empty() {
            rtts.iter().copied().fold(f64::INFINITY, f64::min) // Use minimum RTT
        } else {
            50.0 // Default fallback
        };

        let target_loss = *self.inner.target_loss_pct.lock() / 100.0;
        let rtt_threshold = *self.inner.rtt_floor_ms.lock() as f64;

        // Get current bitrate using detected property
        let current_kbps = self.get_encoder_bitrate(encoder);
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

        // Add dead-band around target loss (±0.1%)
        let loss_deadband = 0.001; // 0.1%
        let loss_too_high = loss_rate > target_loss + loss_deadband;
        let loss_very_low = loss_rate < target_loss - loss_deadband;

        // Adjust based on loss rate and RTT
        if loss_too_high || avg_rtt > rtt_threshold {
            // Decrease bitrate due to high loss or RTT
            new_kbps = current_kbps.saturating_sub(step).max(min);
            gst::info!(
                CAT,
                "Decreasing bitrate from {} to {} kbps (loss={:.2}%, rtt={:.1}ms)",
                current_kbps,
                new_kbps,
                loss_rate * 100.0,
                avg_rtt
            );
        } else if loss_very_low && avg_rtt < rtt_threshold * 0.8 {
            // Increase bitrate due to good conditions (only if loss well below target)
            new_kbps = (current_kbps + step).min(max);
            gst::info!(
                CAT,
                "Increasing bitrate from {} to {} kbps (loss={:.2}%, rtt={:.1}ms)",
                current_kbps,
                new_kbps,
                loss_rate * 100.0,
                avg_rtt
            );
        } else {
            // Within dead-band, no adjustment
            gst::trace!(
                CAT,
                "Bitrate stable at {} kbps (loss={:.2}%, target={:.2}%±{:.1}%, rtt={:.1}ms)",
                current_kbps,
                loss_rate * 100.0,
                target_loss * 100.0,
                loss_deadband * 100.0,
                avg_rtt
            );
        }

        if new_kbps != current_kbps {
            if let Err(e) = self.set_encoder_bitrate(encoder, new_kbps) {
                gst::warning!(CAT, "Failed to set encoder bitrate: {}", e);
            } else {
                *self.inner.last_change.lock() = Some(now);
            }
        }
    }

    fn simple_bitrate_adjustment(&self, encoder: &gst::Element) {
        // Fallback to simple oscillation if no stats
        let current_kbps = self.get_encoder_bitrate(encoder);
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
            gst::info!(
                CAT,
                "Decreasing bitrate from {} to {} kbps (demo mode)",
                current_kbps,
                new_kbps
            );
        } else if current_kbps <= min {
            new_kbps = (current_kbps + step).min(max);
            gst::info!(
                CAT,
                "Increasing bitrate from {} to {} kbps (demo mode)",
                current_kbps,
                new_kbps
            );
        }

        if new_kbps != current_kbps {
            if let Err(e) = self.set_encoder_bitrate(encoder, new_kbps) {
                gst::warning!(CAT, "Failed to set encoder bitrate: {}", e);
            } else {
                *self.inner.last_change.lock() = Some(now);
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
