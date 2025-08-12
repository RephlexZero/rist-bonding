//! Metrics collection and reporting

use crate::types::GeState;
use serde::{Deserialize, Serialize};

/// Per-link metrics snapshot
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LinkMetrics {
    /// Namespace name
    pub namespace: String,
    /// Current egress rate in bps (from OU controller)
    pub egress_rate_bps: u64,
    /// Current Gilbert-Elliott state
    pub ge_state: GeState,
    /// Current loss percentage
    pub loss_pct: f64,
    /// Current delay in ms
    pub delay_ms: u32,
    /// Current jitter in ms
    pub jitter_ms: u32,
    /// Transmitted bytes (from interface stats)
    pub tx_bytes: u64,
    /// Received bytes (from interface stats)
    pub rx_bytes: u64,
    /// Transmitted packets
    pub tx_packets: u64,
    /// Received packets
    pub rx_packets: u64,
    /// Dropped packets
    pub dropped_packets: u64,
}

impl LinkMetrics {
    pub fn new(namespace: String) -> Self {
        Self {
            namespace,
            egress_rate_bps: 0,
            ge_state: GeState::Good,
            loss_pct: 0.0,
            delay_ms: 0,
            jitter_ms: 0,
            tx_bytes: 0,
            rx_bytes: 0,
            tx_packets: 0,
            rx_packets: 0,
            dropped_packets: 0,
        }
    }
}

/// Complete metrics snapshot for all links
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    /// Timestamp in milliseconds since epoch
    pub timestamp_ms: u64,
    /// Per-link metrics
    pub links: Vec<LinkMetrics>,
}

impl MetricsSnapshot {
    pub fn new() -> Self {
        Self {
            timestamp_ms: crate::util::timestamp_ms(),
            links: Vec::new(),
        }
    }

    pub fn add_link(&mut self, metrics: LinkMetrics) {
        self.links.push(metrics);
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn to_json_line(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

/// Metrics collector that can gather statistics from network interfaces
pub struct MetricsCollector {
    rtnetlink_handle: rtnetlink::Handle,
}

impl MetricsCollector {
    pub fn new() -> crate::errors::Result<Self> {
        let (connection, handle, _) = rtnetlink::new_connection()?;
        tokio::spawn(connection);

        Ok(Self {
            rtnetlink_handle: handle,
        })
    }

    /// Collect interface statistics for a given interface index
    pub async fn collect_interface_stats(
        &self,
        if_index: Option<u32>,
    ) -> crate::errors::Result<(u64, u64, u64, u64, u64)> {
        if let Some(index) = if_index {
            self.get_interface_stats(index).await.map(
                |(tx_bytes, rx_bytes, tx_packets, rx_packets)| {
                    (tx_bytes, rx_bytes, tx_packets, rx_packets, 0) // No dropped packets for now
                },
            )
        } else {
            Ok((0, 0, 0, 0, 0))
        }
    }

    /// Collect interface statistics for a given interface index
    async fn get_interface_stats(
        &self,
        if_index: u32,
    ) -> crate::errors::Result<(u64, u64, u64, u64)> {
        use futures::TryStreamExt;

        let mut links = self
            .rtnetlink_handle
            .link()
            .get()
            .match_index(if_index)
            .execute();

        if let Some(_link) = links.try_next().await? {
            // In a full implementation, we'd parse the Stats64 NLA properly
            // For now, return placeholder values since stats parsing is complex
            Ok((0, 0, 0, 0))
        } else {
            Err(crate::errors::NetemError::LinkNotFound(format!(
                "Interface index {}",
                if_index
            )))
        }
    }
}

/// JSONL metrics writer for continuous logging
pub struct MetricsWriter {
    writer: Box<dyn std::io::Write + Send>,
}

impl MetricsWriter {
    pub fn new_file(path: impl AsRef<std::path::Path>) -> std::io::Result<Self> {
        let file = std::fs::File::create(path)?;
        Ok(Self {
            writer: Box::new(file),
        })
    }

    pub fn new_stdout() -> Self {
        Self {
            writer: Box::new(std::io::stdout()),
        }
    }

    pub fn write_snapshot(
        &mut self,
        snapshot: &MetricsSnapshot,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use std::io::Write;

        let json_line = snapshot.to_json_line()?;
        writeln!(self.writer, "{}", json_line)?;
        self.writer.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_serialization() {
        let mut snapshot = MetricsSnapshot::new();
        let metrics = LinkMetrics {
            namespace: "test-ns".to_string(),
            egress_rate_bps: 1000000,
            ge_state: GeState::Good,
            loss_pct: 0.01,
            delay_ms: 20,
            jitter_ms: 5,
            tx_bytes: 1024,
            rx_bytes: 512,
            tx_packets: 10,
            rx_packets: 8,
            dropped_packets: 2,
        };
        snapshot.add_link(metrics);

        let json = snapshot.to_json().expect("JSON serialization should work");
        assert!(json.contains("test-ns"));
        assert!(json.contains("1000000"));

        let json_line = snapshot
            .to_json_line()
            .expect("JSON line serialization should work");
        assert!(!json_line.contains('\n')); // Should be single line
    }
}
