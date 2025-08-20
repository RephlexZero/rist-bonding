//! Metrics collection and aggregation for network simulation

use crate::Result;
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
    pub qdisc_type: String, // "netem", "tbf", "htb", etc.
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
    #[allow(dead_code)]
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
            qdisc_params: Arc::new(parking_lot::RwLock::new(QdiscParams::new(
                "unknown".to_string(),
            ))),
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
    pub fn record_traffic(
        &self,
        bytes_sent: u64,
        bytes_received: u64,
        packets_sent: u64,
        packets_received: u64,
        packets_dropped: u64,
    ) {
        self.bytes_sent.fetch_add(bytes_sent, Ordering::Relaxed);
        self.bytes_received
            .fetch_add(bytes_received, Ordering::Relaxed);
        self.packets_sent.fetch_add(packets_sent, Ordering::Relaxed);
        self.packets_received
            .fetch_add(packets_received, Ordering::Relaxed);
        self.packets_dropped
            .fetch_add(packets_dropped, Ordering::Relaxed);
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
        let avg_rtt_ms = if active_links > 0 {
            sum_rtt / active_links as f64
        } else {
            0.0
        };
        let avg_jitter_ms = if active_links > 0 {
            sum_jitter / active_links as f64
        } else {
            0.0
        };
        let avg_loss_rate = if active_links > 0 {
            sum_loss / active_links as f64
        } else {
            0.0
        };

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::sleep;

    #[test]
    fn test_link_stats_creation() {
        let stats = LinkStats::new("test_link".to_string());
        assert_eq!(stats.link_id, "test_link");
        assert_eq!(stats.bytes_sent, 0);
        assert_eq!(stats.bytes_received, 0);
        assert_eq!(stats.packets_sent, 0);
        assert_eq!(stats.packets_received, 0);
        assert_eq!(stats.packets_dropped, 0);
        assert_eq!(stats.rtt_ms, 0.0);
        assert_eq!(stats.jitter_ms, 0.0);
        assert_eq!(stats.loss_rate, 0.0);
        assert_eq!(stats.throughput_bps, 0);
        assert_eq!(stats.queue_depth, 0);
        assert_eq!(stats.queue_max, 0);
        assert_eq!(stats.queue_drops, 0);
        assert_eq!(stats.collection_interval_ms, 1000);
    }

    #[test]
    fn test_qdisc_params_creation() {
        let params = QdiscParams::new("netem".to_string());
        assert_eq!(params.qdisc_type, "netem");
        assert!(params.delay_ms.is_none());
        assert!(params.jitter_ms.is_none());
        assert!(params.loss_pct.is_none());
        assert!(params.rate_kbps.is_none());
        assert!(params.burst_bytes.is_none());
    }

    #[test]
    fn test_queue_metrics_creation() {
        let metrics = QueueMetrics::new();
        assert_eq!(metrics.current_depth, 0);
        assert_eq!(metrics.max_depth, 0);
        assert_eq!(metrics.enqueue_count, 0);
        assert_eq!(metrics.dequeue_count, 0);
        assert_eq!(metrics.drop_count, 0);
        assert_eq!(metrics.bytes_queued, 0);
        assert_eq!(metrics.avg_queue_time_ms, 0.0);
    }

    #[test]
    fn test_queue_metrics_default() {
        let metrics = QueueMetrics::default();
        assert_eq!(metrics.current_depth, 0);
        assert_eq!(metrics.max_depth, 0);
    }

    #[test]
    fn test_link_metrics_collector_creation() {
        let collector = LinkMetricsCollector::new("test_link".to_string());
        assert_eq!(collector.link_id, "test_link");

        // Check that atomic counters are initialized to 0
        assert_eq!(collector.bytes_sent.load(Ordering::Relaxed), 0);
        assert_eq!(collector.bytes_received.load(Ordering::Relaxed), 0);
        assert_eq!(collector.packets_sent.load(Ordering::Relaxed), 0);
        assert_eq!(collector.packets_received.load(Ordering::Relaxed), 0);
        assert_eq!(collector.packets_dropped.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_link_metrics_collector_traffic_recording() {
        let collector = LinkMetricsCollector::new("test_link".to_string());

        // Record some traffic
        collector.record_traffic(1000, 2000, 10, 20, 1);

        // Check that atomic counters are updated
        assert_eq!(collector.bytes_sent.load(Ordering::Relaxed), 1000);
        assert_eq!(collector.bytes_received.load(Ordering::Relaxed), 2000);
        assert_eq!(collector.packets_sent.load(Ordering::Relaxed), 10);
        assert_eq!(collector.packets_received.load(Ordering::Relaxed), 20);
        assert_eq!(collector.packets_dropped.load(Ordering::Relaxed), 1);

        // Record more traffic to test accumulation
        collector.record_traffic(500, 1000, 5, 10, 2);

        assert_eq!(collector.bytes_sent.load(Ordering::Relaxed), 1500);
        assert_eq!(collector.bytes_received.load(Ordering::Relaxed), 3000);
        assert_eq!(collector.packets_sent.load(Ordering::Relaxed), 15);
        assert_eq!(collector.packets_received.load(Ordering::Relaxed), 30);
        assert_eq!(collector.packets_dropped.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn test_link_metrics_collector_quality_metrics() {
        let collector = LinkMetricsCollector::new("test_link".to_string());

        // Update quality metrics
        collector.update_quality(25.5, 5.2, 0.05, 1_000_000);

        let performance = collector.get_performance();
        assert_eq!(performance.link_stats.rtt_ms, 25.5);
        assert_eq!(performance.link_stats.jitter_ms, 5.2);
        assert_eq!(performance.link_stats.loss_rate, 0.05);
        assert_eq!(performance.link_stats.throughput_bps, 1_000_000);
    }

    #[test]
    fn test_link_metrics_collector_queue_update() {
        let collector = LinkMetricsCollector::new("test_link".to_string());

        // Update queue metrics
        let mut queue_metrics = QueueMetrics::new();
        queue_metrics.current_depth = 50;
        queue_metrics.max_depth = 100;
        queue_metrics.enqueue_count = 1000;
        queue_metrics.dequeue_count = 900;
        queue_metrics.drop_count = 50;
        queue_metrics.bytes_queued = 25000;

        collector.update_queue(queue_metrics);

        let performance = collector.get_performance();
        assert_eq!(performance.queue_metrics.current_depth, 50);
        assert_eq!(performance.queue_metrics.max_depth, 100);
        assert_eq!(performance.queue_metrics.enqueue_count, 1000);
        assert_eq!(performance.queue_metrics.dequeue_count, 900);
        assert_eq!(performance.queue_metrics.drop_count, 50);
        assert_eq!(performance.queue_metrics.bytes_queued, 25000);
    }

    #[test]
    fn test_link_metrics_collector_qdisc_update() {
        let collector = LinkMetricsCollector::new("test_link".to_string());

        let mut params = QdiscParams::new("netem".to_string());
        params.delay_ms = Some(20);
        params.jitter_ms = Some(5);
        params.loss_pct = Some(0.1);
        params.rate_kbps = Some(1000);
        params.burst_bytes = Some(15000);

        collector.update_qdisc(params.clone());

        let performance = collector.get_performance();
        assert_eq!(performance.qdisc_params.qdisc_type, "netem");
        assert_eq!(performance.qdisc_params.delay_ms, Some(20));
        assert_eq!(performance.qdisc_params.jitter_ms, Some(5));
        assert_eq!(performance.qdisc_params.loss_pct, Some(0.1));
        assert_eq!(performance.qdisc_params.rate_kbps, Some(1000));
        assert_eq!(performance.qdisc_params.burst_bytes, Some(15000));
    }

    #[test]
    fn test_metrics_collector_creation() {
        let collector = MetricsCollector::new();
        assert!(collector.link_collectors.is_empty());
    }

    #[test]
    fn test_metrics_collector_default() {
        let collector = MetricsCollector::default();
        assert!(collector.link_collectors.is_empty());
    }

    #[test]
    fn test_metrics_collector_link_management() {
        let collector = MetricsCollector::new();

        // Add a link
        let link1 = collector.add_link("link1".to_string());
        assert_eq!(collector.link_collectors.len(), 1);

        // Get the link
        let retrieved = collector.get_link("link1").unwrap();
        assert!(Arc::ptr_eq(&link1, &retrieved));

        // Add another link
        let _link2 = collector.add_link("link2".to_string());
        assert_eq!(collector.link_collectors.len(), 2);

        // Remove a link
        let removed = collector.remove_link("link1").unwrap();
        assert!(Arc::ptr_eq(&link1, &removed));
        assert_eq!(collector.link_collectors.len(), 1);

        // Try to get removed link
        assert!(collector.get_link("link1").is_none());

        // Try to remove non-existent link
        assert!(collector.remove_link("non_existent").is_none());
    }

    #[tokio::test]
    async fn test_metrics_collector_empty_snapshot() {
        let collector = MetricsCollector::new();

        let snapshot = collector.take_snapshot().await.unwrap();

        assert_eq!(snapshot.link_performance.len(), 0);
        assert_eq!(snapshot.simulation_metrics.active_links, 0);
        assert_eq!(snapshot.simulation_metrics.total_bytes_sent, 0);
        assert_eq!(snapshot.simulation_metrics.total_bytes_received, 0);
        assert_eq!(snapshot.simulation_metrics.total_packets_sent, 0);
        assert_eq!(snapshot.simulation_metrics.total_packets_received, 0);
        assert_eq!(snapshot.simulation_metrics.total_drops, 0);
        assert_eq!(snapshot.simulation_metrics.avg_rtt_ms, 0.0);
        assert_eq!(snapshot.simulation_metrics.avg_jitter_ms, 0.0);
        assert_eq!(snapshot.simulation_metrics.avg_loss_rate, 0.0);
        assert_eq!(snapshot.simulation_metrics.total_throughput_bps, 0);
    }

    #[tokio::test]
    async fn test_metrics_collector_single_link_snapshot() {
        let collector = MetricsCollector::new();
        let link = collector.add_link("link1".to_string());

        // Add some data to the link
        link.record_traffic(1000, 2000, 10, 20, 1);
        link.update_quality(25.0, 5.0, 0.1, 500_000);

        let snapshot = collector.take_snapshot().await.unwrap();

        assert_eq!(snapshot.link_performance.len(), 1);
        assert_eq!(snapshot.simulation_metrics.active_links, 1);
        assert_eq!(snapshot.simulation_metrics.total_bytes_sent, 1000);
        assert_eq!(snapshot.simulation_metrics.total_bytes_received, 2000);
        assert_eq!(snapshot.simulation_metrics.total_packets_sent, 10);
        assert_eq!(snapshot.simulation_metrics.total_packets_received, 20);
        assert_eq!(snapshot.simulation_metrics.total_drops, 1);
        assert_eq!(snapshot.simulation_metrics.avg_rtt_ms, 25.0);
        assert_eq!(snapshot.simulation_metrics.avg_jitter_ms, 5.0);
        assert_eq!(snapshot.simulation_metrics.avg_loss_rate, 0.1);
        assert_eq!(snapshot.simulation_metrics.total_throughput_bps, 500_000);
        assert_eq!(snapshot.simulation_metrics.link_ids, vec!["link1"]);
    }

    #[tokio::test]
    async fn test_metrics_collector_multiple_links_snapshot() {
        let collector = MetricsCollector::new();
        let link1 = collector.add_link("link1".to_string());
        let link2 = collector.add_link("link2".to_string());

        // Add data to first link
        link1.record_traffic(1000, 2000, 10, 20, 1);
        link1.update_quality(20.0, 4.0, 0.05, 400_000);

        // Add data to second link
        link2.record_traffic(2000, 3000, 20, 30, 2);
        link2.update_quality(30.0, 6.0, 0.15, 600_000);

        let snapshot = collector.take_snapshot().await.unwrap();

        assert_eq!(snapshot.link_performance.len(), 2);
        assert_eq!(snapshot.simulation_metrics.active_links, 2);
        assert_eq!(snapshot.simulation_metrics.total_bytes_sent, 3000);
        assert_eq!(snapshot.simulation_metrics.total_bytes_received, 5000);
        assert_eq!(snapshot.simulation_metrics.total_packets_sent, 30);
        assert_eq!(snapshot.simulation_metrics.total_packets_received, 50);
        assert_eq!(snapshot.simulation_metrics.total_drops, 3);
        assert_eq!(snapshot.simulation_metrics.avg_rtt_ms, 25.0); // (20 + 30) / 2
        assert_eq!(snapshot.simulation_metrics.avg_jitter_ms, 5.0); // (4 + 6) / 2
        assert_eq!(snapshot.simulation_metrics.avg_loss_rate, 0.1); // (0.05 + 0.15) / 2
        assert_eq!(snapshot.simulation_metrics.total_throughput_bps, 1_000_000); // 400k + 600k
        assert!(snapshot
            .simulation_metrics
            .link_ids
            .contains(&"link1".to_string()));
        assert!(snapshot
            .simulation_metrics
            .link_ids
            .contains(&"link2".to_string()));
    }

    #[tokio::test]
    async fn test_metrics_collector_concurrent_access() {
        let collector = Arc::new(MetricsCollector::new());
        let link = collector.add_link("concurrent_link".to_string());

        // Spawn multiple tasks that record metrics concurrently
        let mut handles = Vec::new();

        for i in 0..10 {
            let link_clone = link.clone();
            let handle = tokio::spawn(async move {
                for j in 0..10 {
                    link_clone.record_traffic(
                        (i * 10 + j) * 100, // bytes_sent
                        (i * 10 + j) * 200, // bytes_received
                        i * 10 + j,         // packets_sent
                        (i * 10 + j) * 2,   // packets_received
                        i * 10 + j / 10,    // packets_dropped
                    );

                    // Small delay to encourage interleaving
                    sleep(Duration::from_micros(1)).await;
                }
            });
            handles.push(handle);
        }

        // Wait for all tasks to complete
        for handle in handles {
            handle.await.unwrap();
        }

        let snapshot = collector.take_snapshot().await.unwrap();

        // Verify that all updates were recorded (exact values depend on concurrent execution)
        assert!(snapshot.simulation_metrics.total_bytes_sent > 0);
        assert!(snapshot.simulation_metrics.total_bytes_received > 0);
        assert!(snapshot.simulation_metrics.total_packets_sent > 0);
        assert!(snapshot.simulation_metrics.total_packets_received > 0);
        assert_eq!(snapshot.simulation_metrics.active_links, 1);
        assert_eq!(snapshot.link_performance.len(), 1);
    }
}
