use gst::glib;
use gstreamer as gst;
use once_cell::sync::Lazy;

use glib::ParamSpecBuilderExt;

pub(crate) fn properties() -> &'static [glib::ParamSpec] {
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
                .default_value(true)
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
                .flags(glib::ParamFlags::READABLE)
                .blurb("Current weight values as JSON array - readonly for monitoring")
                .build(),
            glib::ParamSpecUInt64::builder("min-hold-ms")
                .nick("Minimum hold time (ms)")
                .blurb("Minimum time between pad switches to prevent thrashing")
                .minimum(0)
                .maximum(10000)
                .default_value(200)
                .build(),
            glib::ParamSpecDouble::builder("switch-threshold")
                .nick("Switch threshold ratio")
                .blurb("Minimum weight ratio required to switch pads (new_weight/current_weight)")
                .minimum(1.0)
                .maximum(10.0)
                .default_value(1.05)
                .build(),
            glib::ParamSpecUInt64::builder("health-warmup-ms")
                .nick("Health warmup time (ms)")
                .blurb("Time for new link health metrics to stabilize before full consideration")
                .minimum(0)
                .maximum(30000)
                .default_value(2000)
                .build(),
            glib::ParamSpecBoolean::builder("duplicate-keyframes")
                .nick("Duplicate keyframes")
                .blurb("Enable selective keyframe duplication during failover windows")
                .default_value(false)
                .build(),
            glib::ParamSpecUInt::builder("dup-budget-pps")
                .nick("Duplication budget (packets/sec)")
                .blurb("Maximum keyframe duplications per second")
                .minimum(0)
                .maximum(100)
                .default_value(5)
                .build(),
            glib::ParamSpecUInt64::builder("metrics-export-interval-ms")
                .nick("Metrics export interval (ms)")
                .blurb("How often to emit metrics bus messages in milliseconds (0 = disabled)")
                .minimum(0)
                .maximum(60000)
                .default_value(0)
                .build(),
            glib::ParamSpecDouble::builder("ewma-rtx-penalty")
                .nick("EWMA RTX penalty coefficient")
                .blurb("Alpha coefficient for retransmission-rate penalty in EWMA weighting")
                .minimum(0.0)
                .maximum(10.0)
                .default_value(0.3)
                .build(),
            glib::ParamSpecDouble::builder("ewma-rtt-penalty")
                .nick("EWMA RTT penalty coefficient")
                .blurb("Beta coefficient for RTT penalty in EWMA weighting")
                .minimum(0.0)
                .maximum(10.0)
                .default_value(0.1)
                .build(),
            glib::ParamSpecDouble::builder("aimd-rtx-threshold")
                .nick("AIMD RTX threshold")
                .blurb("RTX rate threshold for multiplicative decrease in AIMD strategy")
                .minimum(0.0)
                .maximum(1.0)
                .default_value(0.05)
                .build(),
            glib::ParamSpecDouble::builder("probe-ratio")
                .nick("Exploration probe ratio")
                .blurb("Deterministic epsilon mix applied after normalization (0.0-0.5)")
                .minimum(0.0)
                .maximum(0.5)
                .default_value(0.08)
                .build(),
            glib::ParamSpecDouble::builder("max-link-share")
                .nick("Max link share cap")
                .blurb("Hard cap for any single normalized weight before epsilon mix (<=1.0, 1.0 disables)")
                .minimum(0.5)
                .maximum(1.0)
                .default_value(0.70)
                .build(),
            glib::ParamSpecDouble::builder("probe-boost")
                .nick("Probe weight boost")
                .blurb("Multiplicative weight boost applied to the rotating probe target before normalization")
                .minimum(0.0)
                .maximum(1.0)
                .default_value(0.12)
                .build(),
            glib::ParamSpecUInt64::builder("probe-period-ms")
                .nick("Probe rotation period (ms)")
                .blurb("How often to rotate the micro-probe target index")
                .minimum(200)
                .maximum(10000)
                .default_value(800)
                .build(),
            glib::ParamSpecString::builder("scheduler")
                .nick("Scheduler mode")
                .blurb("Packet scheduler: 'swrr' (packets) or 'drr' (bytes)")
                .default_value(Some("swrr"))
                .build(),
            glib::ParamSpecUInt::builder("quantum-bytes")
                .nick("DRR base quantum (bytes)")
                .blurb("Base quantum used by DRR per round before weight scaling")
                .minimum(256)
                .maximum(16384)
                .default_value(1500)
                .build(),
        ]
    });
    PROPS.as_ref()
}
