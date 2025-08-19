//! Metrics collection and aggregation for network simulation

use crate::{Result, ObservabilityError};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use uuid::Uuid;

/// Statistics for a single network link
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkStats {
    pub link_id: String,
    pub interface_name: Option<String>,
    
    // Traffic counters
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub packets_sent: u64,
    pub packets_received: u64,
    pub packets_dropped: u64,
    
    // Quality metrics
    pub rtt_ms: f64,
    pub jitter_ms: f64,
    pub loss_rate: f64,
    pub throughput_bps: u64,
    
    // Queue metrics
    pub queue_depth: usize,
    pub queue_max: usize,
    pub queue_drops: u64,
    
    // Timestamps
    pub last_updated: DateTime<Utc>,
    pub collection_interval_ms: u64,
}

impl LinkStats {
    pub fn new(link_id: String) -> Self {
        Self {
            link_id,
            interface_name: None,
            bytes_sent: 0,
            bytes_received: 0,
            packets_sent: 0,
            packets_received: 0,
            packets_dropped: 0,
            rtt_ms: 0.0,
            jitter_ms: 0.0,
            loss_rate: 0.0,
            throughput_bps: 0,
            queue_depth: 0,
            queue_max: 0,
            queue_drops: 0,
            last_updated: Utc::now(),
            collection_interval_ms: 1000,
        }
    }
}

/// Current qdisc parameters applied to a link
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QdiscParams {
    pub qdisc_type: String,  // "netem", "tbf", "htb", etc.
    pub delay_ms: Option<u64>,
    pub jitter_ms: Option<u64>,
    pub loss_pct: Option<f32>,
    pub rate_kbps: Option<u64>,
    pub burst_bytes: Option<usize>,
    pub last_changed: DateTime<Utc>,
}

impl QdiscParams {
    pub fn new(qdisc_type: String) -> Self {
        Self {
            qdisc_type,
            delay_ms: None,
            jitter_ms: None,
            loss_pct: None,
            rate_kbps: None,
            burst_bytes: None,
            last_changed: Utc::now(),
        }
    }
}

/// Real-time queue metrics for a link
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueMetrics {
    pub current_depth: usize,
    pub max_depth: usize,
    pub enqueue_count: u64,
    pub dequeue_count: u64,
    pub drop_count: u64,
    pub bytes_queued: u64,
    pub avg_queue_time_ms: f64,
    pub last_updated: DateTime<Utc>,
}

impl QueueMetrics {
    pub fn new() -> Self {
        Self {
            current_depth: 0,
            max_depth: 0,
            enqueue_count: 0,
            dequeue_count: 0,
            drop_count: 0,
            bytes_queued: 0,
            avg_queue_time_ms: 0.0,
            last_updated: Utc::now(),
        }
    }
}

/// Combined performance metrics for a link
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkPerformance {
    pub link_stats: LinkStats,
    pub qdisc_params: QdiscParams,
    pub queue_metrics: QueueMetrics,
}

/// Aggregated simulation metrics across all links
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationMetrics {
    pub simulation_id: Uuid,
    pub start_time: DateTime<Utc>,
    pub duration_ms: u64,
    
    // Aggregate counters
    pub total_bytes_sent: u64,
    pub total_bytes_received: u64,
    pub total_packets_sent: u64,
    pub total_packets_received: u64,
    pub total_drops: u64,
    
    // Quality aggregates
    pub avg_rtt_ms: f64,
    pub avg_jitter_ms: f64,
    pub avg_loss_rate: f64,
    pub total_throughput_bps: u64,
    
    // Active links
    pub active_links: usize,
    pub link_ids: Vec<String>,
}

/// Complete metrics snapshot at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub timestamp: DateTime<Utc>,
    pub simulation_metrics: SimulationMetrics,
    pub link_performance: Vec<LinkPerformance>,
    pub metadata: serde_json::Value,
}

/// Collects metrics for individual links
pub struct LinkMetricsCollector {
    link_id: String,
    stats: Arc<parking_lot::RwLock<LinkStats>>,
    qdisc_params: Arc<parking_lot::RwLock<QdiscParams>>,
    queue_metrics: Arc<parking_lot::RwLock<QueueMetrics>>,
    
    // Atomic counters for thread-safe updates
    bytes_sent: AtomicU64,
    bytes_received: AtomicU64,
    packets_sent: AtomicU64,
    packets_received: AtomicU64,
    packets_dropped: AtomicU64,
}

impl LinkMetricsCollector {
    pub fn new(link_id: String) -> Self {
        Self {
            stats: Arc::new(parking_lot::RwLock::new(LinkStats::new(link_id.clone()))),
            qdisc_params: Arc::new(parking_lot::RwLock::new(QdiscParams::new("unknown".to_string()))),
            queue_metrics: Arc::new(parking_lot::RwLock::new(QueueMetrics::new())),
            link_id,
            bytes_sent: AtomicU64::new(0),
            bytes_received: AtomicU64::new(0),
            packets_sent: AtomicU64::new(0),
            packets_received: AtomicU64::new(0),
            packets_dropped: AtomicU64::new(0),
        }
    }

    /// Record traffic statistics
    pub fn record_traffic(&self, bytes_sent: u64, bytes_received: u64, packets_sent: u64, packets_received: u64, packets_dropped: u64) {
        self.bytes_sent.fetch_add(bytes_sent, Ordering::Relaxed);
        self.bytes_received.fetch_add(bytes_received, Ordering::Relaxed);
        self.packets_sent.fetch_add(packets_sent, Ordering::Relaxed);
        self.packets_received.fetch_add(packets_received, Ordering::Relaxed);
        self.packets_dropped.fetch_add(packets_dropped, Ordering::Relaxed);
    }

    /// Update quality metrics (RTT, jitter, loss rate, throughput)
    pub fn update_quality(&self, rtt_ms: f64, jitter_ms: f64, loss_rate: f64, throughput_bps: u64) {
        let mut stats = self.stats.write();
        stats.rtt_ms = rtt_ms;
        stats.jitter_ms = jitter_ms;
        stats.loss_rate = loss_rate;
        stats.throughput_bps = throughput_bps;
        stats.last_updated = Utc::now();
    }

    /// Update qdisc parameters when they change
    pub fn update_qdisc(&self, params: QdiscParams) {
        *self.qdisc_params.write() = params;
    }

    /// Update queue metrics
    pub fn update_queue(&self, metrics: QueueMetrics) {
        *self.queue_metrics.write() = metrics;
    }

    /// Get current link performance snapshot
    pub fn get_performance(&self) -> LinkPerformance {
        // Update stats with atomic counters
        {
            let mut stats = self.stats.write();
            stats.bytes_sent = self.bytes_sent.load(Ordering::Relaxed);
            stats.bytes_received = self.bytes_received.load(Ordering::Relaxed);
            stats.packets_sent = self.packets_sent.load(Ordering::Relaxed);
            stats.packets_received = self.packets_received.load(Ordering::Relaxed);
            stats.packets_dropped = self.packets_dropped.load(Ordering::Relaxed);
        }

        LinkPerformance {
            link_stats: self.stats.read().clone(),
            qdisc_params: self.qdisc_params.read().clone(),
            queue_metrics: self.queue_metrics.read().clone(),
        }
    }
}

/// Main metrics collector that aggregates data from all links
pub struct MetricsCollector {
    simulation_id: Uuid,
    start_time: DateTime<Utc>,
    link_collectors: DashMap<String, Arc<LinkMetricsCollector>>,
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            simulation_id: Uuid::new_v4(),
            start_time: Utc::now(),
            link_collectors: DashMap::new(),
        }
    }

    /// Add a new link to collect metrics for
    pub fn add_link(&self, link_id: String) -> Arc<LinkMetricsCollector> {
        let collector = Arc::new(LinkMetricsCollector::new(link_id.clone()));
        self.link_collectors.insert(link_id, collector.clone());
        collector
    }

    /// Remove a link from collection
    pub fn remove_link(&self, link_id: &str) -> Option<Arc<LinkMetricsCollector>> {
        self.link_collectors.remove(link_id).map(|(_, v)| v)
    }

    /// Get metrics collector for a specific link
    pub fn get_link(&self, link_id: &str) -> Option<Arc<LinkMetricsCollector>> {
        self.link_collectors.get(link_id).map(|r| r.value().clone())
    }

    /// Take a complete metrics snapshot
    pub async fn take_snapshot(&self) -> Result<MetricsSnapshot> {
        let timestamp = Utc::now();
        let duration_ms = (timestamp - self.start_time).num_milliseconds() as u64;

        // Collect all link performance data
        let mut link_performance = Vec::new();
        let mut total_bytes_sent = 0;
        let mut total_bytes_received = 0;
        let mut total_packets_sent = 0;
        let mut total_packets_received = 0;
        let mut total_drops = 0;
        let mut total_throughput_bps = 0;
        let mut sum_rtt = 0.0;
        let mut sum_jitter = 0.0;
        let mut sum_loss = 0.0;
        let mut link_ids = Vec::new();

        for link_ref in self.link_collectors.iter() {
            let performance = link_ref.value().get_performance();
            
            total_bytes_sent += performance.link_stats.bytes_sent;
            total_bytes_received += performance.link_stats.bytes_received;
            total_packets_sent += performance.link_stats.packets_sent;
            total_packets_received += performance.link_stats.packets_received;
            total_drops += performance.link_stats.packets_dropped;
            total_throughput_bps += performance.link_stats.throughput_bps;
            
            sum_rtt += performance.link_stats.rtt_ms;
            sum_jitter += performance.link_stats.jitter_ms;
            sum_loss += performance.link_stats.loss_rate;
            
            link_ids.push(performance.link_stats.link_id.clone());
            link_performance.push(performance);
        }

        let active_links = link_performance.len();
        let avg_rtt_ms = if active_links > 0 { sum_rtt / active_links as f64 } else { 0.0 };
        let avg_jitter_ms = if active_links > 0 { sum_jitter / active_links as f64 } else { 0.0 };
        let avg_loss_rate = if active_links > 0 { sum_loss / active_links as f64 } else { 0.0 };

        let simulation_metrics = SimulationMetrics {
            simulation_id: self.simulation_id,
            start_time: self.start_time,
            duration_ms,
            total_bytes_sent,
            total_bytes_received,
            total_packets_sent,
            total_packets_received,
            total_drops,
            avg_rtt_ms,
            avg_jitter_ms,
            avg_loss_rate,
            total_throughput_bps,
            active_links,
            link_ids,
        };

        Ok(MetricsSnapshot {
            timestamp,
            simulation_metrics,
            link_performance,
            metadata: serde_json::json!({
                "collector_version": "1.0.0",
                "runtime": "tokio"
            }),
        })
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for QueueMetrics {
    fn default() -> Self {
        Self::new()
    }
}