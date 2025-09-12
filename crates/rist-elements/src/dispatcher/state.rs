use gst::glib;
use gstreamer as gst;
use parking_lot::Mutex;

#[derive(Debug, Clone)]
pub struct LinkStats {
    pub prev_sent_original: u64,
    pub prev_sent_retransmitted: u64,
    pub prev_timestamp: std::time::Instant,
    pub ewma_goodput: f64,
    pub prev_rr_received: u64,
    pub ewma_delivered_pps: f64,
    pub ewma_rtx_rate: f64,
    pub ewma_rtt: f64,
    pub alpha: f64,
}

impl Default for LinkStats {
    fn default() -> Self {
        Self {
            prev_sent_original: 0,
            prev_sent_retransmitted: 0,
            prev_timestamp: std::time::Instant::now(),
            ewma_goodput: 0.0,
            prev_rr_received: 0,
            ewma_delivered_pps: 0.0,
            ewma_rtx_rate: 0.0,
            ewma_rtt: 50.0,
            alpha: 0.25,
        }
    }
}

pub struct State {
    pub next_out: usize,
    pub weights: Vec<f64>,
    pub swrr_counters: Vec<f64>,
    pub drr_deficits: Vec<i64>,
    pub drr_ptr: usize,
    pub drr_current_burst: usize,
    pub drr_last_selected: usize,
    pub cached_stream_start: Option<gst::Event>,
    pub cached_caps: Option<gst::Event>,
    pub cached_segment: Option<gst::Event>,
    pub cached_tags: Vec<gst::Event>,
    pub link_stats: Vec<LinkStats>,
    pub last_switch_time: Option<std::time::Instant>,
    pub link_health_timers: Vec<std::time::Instant>,
    pub dup_budget_used: u32,
    pub dup_budget_reset_time: Option<std::time::Instant>,
    pub started_at: std::time::Instant,
    pub probe_idx: usize,
    pub last_probe: std::time::Instant,
    pub orig_packets: u64,
    pub last_flow_check_packets: u64,
    pub last_flow_check_time: std::time::Instant,
    pub last_buffer_time: std::time::Instant,
}

impl Default for State {
    fn default() -> Self {
        Self {
            next_out: 0,
            weights: Vec::new(),
            swrr_counters: Vec::new(),
            drr_deficits: Vec::new(),
            drr_ptr: 0,
            drr_current_burst: 0,
            drr_last_selected: 0,
            cached_stream_start: None,
            cached_caps: None,
            cached_segment: None,
            cached_tags: Vec::new(),
            link_stats: Vec::new(),
            last_switch_time: None,
            link_health_timers: Vec::new(),
            dup_budget_used: 0,
            dup_budget_reset_time: None,
            started_at: std::time::Instant::now(),
            probe_idx: 0,
            last_probe: std::time::Instant::now(),
            orig_packets: 0,
            last_flow_check_packets: 0,
            last_flow_check_time: std::time::Instant::now(),
            last_buffer_time: std::time::Instant::now(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Strategy {
    Aimd,
    #[default]
    Ewma,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scheduler {
    #[default]
    Swrr,
    Drr,
}

pub struct DispatcherInner {
    pub state: Mutex<State>,
    pub sinkpad: Mutex<Option<gst::Pad>>,
    pub srcpads: Mutex<Vec<gst::Pad>>,
    pub srcpad_counter: Mutex<usize>,
    pub rebalance_interval_ms: Mutex<u64>,
    pub strategy: Mutex<Strategy>,
    pub caps_any: Mutex<bool>,
    pub auto_balance: Mutex<bool>,
    pub min_hold_ms: Mutex<u64>,
    pub switch_threshold: Mutex<f64>,
    pub health_warmup_ms: Mutex<u64>,
    pub duplicate_keyframes: Mutex<bool>,
    pub dup_budget_pps: Mutex<u32>,
    pub metrics_export_interval_ms: Mutex<u64>,
    pub metrics_timeout_id: Mutex<Option<glib::SourceId>>,
    pub rist_element: Mutex<Option<gst::Element>>,
    pub stats_timeout_id: Mutex<Option<glib::SourceId>>,
    pub ewma_rtx_penalty: Mutex<f64>,
    pub ewma_rtt_penalty: Mutex<f64>,
    pub aimd_rtx_threshold: Mutex<f64>,
    pub probe_ratio: Mutex<f64>,
    pub max_link_share: Mutex<f64>,
    pub probe_boost: Mutex<f64>,
    pub probe_period_ms: Mutex<u64>,
    pub scheduler: Mutex<Scheduler>,
    pub quantum_bytes: Mutex<u32>,
    pub min_burst_pkts: Mutex<u32>,
    pub use_switch_threshold: Mutex<bool>,
    pub flow_watchdog_id: Mutex<Option<glib::SourceId>>,
}

impl Default for DispatcherInner {
    fn default() -> Self {
        Self {
            state: Mutex::new(State::default()),
            sinkpad: Mutex::new(None),
            srcpads: Mutex::new(Vec::new()),
            srcpad_counter: Mutex::new(0),
            rebalance_interval_ms: Mutex::new(500),
            strategy: Mutex::new(Strategy::default()),
            caps_any: Mutex::new(false),
            auto_balance: Mutex::new(true),
            min_hold_ms: Mutex::new(200),
            switch_threshold: Mutex::new(1.05),
            health_warmup_ms: Mutex::new(2000),
            duplicate_keyframes: Mutex::new(false),
            dup_budget_pps: Mutex::new(5),
            metrics_export_interval_ms: Mutex::new(0),
            metrics_timeout_id: Mutex::new(None),
            rist_element: Mutex::new(None),
            stats_timeout_id: Mutex::new(None),
            ewma_rtx_penalty: Mutex::new(0.3),
            ewma_rtt_penalty: Mutex::new(0.1),
            aimd_rtx_threshold: Mutex::new(0.05),
            probe_ratio: Mutex::new(0.08),
            max_link_share: Mutex::new(0.70),
            probe_boost: Mutex::new(0.12),
            probe_period_ms: Mutex::new(800),
            scheduler: Mutex::new(Scheduler::Swrr),
            quantum_bytes: Mutex::new(1200),
            min_burst_pkts: Mutex::new(12),
            use_switch_threshold: Mutex::new(false),
            flow_watchdog_id: Mutex::new(None),
        }
    }
}
