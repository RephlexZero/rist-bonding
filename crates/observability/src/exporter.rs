//! Metrics exporters for different output formats

use crate::{MetricsSnapshot, Result};
use async_trait::async_trait;
use serde_json;
use std::path::Path;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

/// Trait for exporting metrics snapshots in different formats
#[async_trait]
pub trait MetricsExporter {
    async fn export(&self, snapshot: &MetricsSnapshot) -> Result<String>;
    async fn export_to_file(&self, snapshot: &MetricsSnapshot, path: &Path) -> Result<()>;
}

/// Exports metrics in Prometheus format
pub struct PrometheusExporter {
    namespace: String,
}

impl PrometheusExporter {
    pub fn new() -> Result<Self> {
        Ok(Self {
            namespace: "rist_simulation".to_string(),
        })
    }

    pub fn with_namespace(namespace: String) -> Self {
        Self { namespace }
    }

    fn format_prometheus_line(
        &self,
        name: &str,
        value: f64,
        labels: &[(&str, &str)],
        help: &str,
    ) -> String {
        let mut result = String::new();

        // Add help comment
        result.push_str(&format!("# HELP {}{} {}\n", self.namespace, name, help));
        result.push_str(&format!("# TYPE {}{} gauge\n", self.namespace, name));

        // Format labels
        let label_str = if labels.is_empty() {
            String::new()
        } else {
            let labels_formatted: Vec<String> = labels
                .iter()
                .map(|(k, v)| format!("{}=\"{}\"", k, v))
                .collect();
            format!("{{{}}}", labels_formatted.join(","))
        };

        result.push_str(&format!(
            "{}{}{} {}\n",
            self.namespace, name, label_str, value
        ));
        result
    }
}

#[async_trait]
impl MetricsExporter for PrometheusExporter {
    async fn export(&self, snapshot: &MetricsSnapshot) -> Result<String> {
        let mut output = String::new();

        // Simulation-level metrics
        let sim = &snapshot.simulation_metrics;
        output.push_str(&self.format_prometheus_line(
            "_simulation_duration_ms",
            sim.duration_ms as f64,
            &[("simulation_id", &sim.simulation_id.to_string())],
            "Total simulation duration in milliseconds",
        ));

        output.push_str(&self.format_prometheus_line(
            "_total_bytes_sent",
            sim.total_bytes_sent as f64,
            &[("simulation_id", &sim.simulation_id.to_string())],
            "Total bytes sent across all links",
        ));

        output.push_str(&self.format_prometheus_line(
            "_total_throughput_bps",
            sim.total_throughput_bps as f64,
            &[("simulation_id", &sim.simulation_id.to_string())],
            "Total throughput in bits per second",
        ));

        output.push_str(&self.format_prometheus_line(
            "_avg_rtt_ms",
            sim.avg_rtt_ms,
            &[("simulation_id", &sim.simulation_id.to_string())],
            "Average round-trip time across all links",
        ));

        output.push_str(&self.format_prometheus_line(
            "_avg_loss_rate",
            sim.avg_loss_rate,
            &[("simulation_id", &sim.simulation_id.to_string())],
            "Average packet loss rate across all links",
        ));

        // Per-link metrics
        for link in &snapshot.link_performance {
            let link_id = &link.link_stats.link_id;

            output.push_str(&self.format_prometheus_line(
                "_link_bytes_sent",
                link.link_stats.bytes_sent as f64,
                &[("link_id", link_id)],
                "Bytes sent on this link",
            ));

            output.push_str(&self.format_prometheus_line(
                "_link_bytes_received",
                link.link_stats.bytes_received as f64,
                &[("link_id", link_id)],
                "Bytes received on this link",
            ));

            output.push_str(&self.format_prometheus_line(
                "_link_rtt_ms",
                link.link_stats.rtt_ms,
                &[("link_id", link_id)],
                "Round-trip time for this link",
            ));

            output.push_str(&self.format_prometheus_line(
                "_link_jitter_ms",
                link.link_stats.jitter_ms,
                &[("link_id", link_id)],
                "Jitter for this link",
            ));

            output.push_str(&self.format_prometheus_line(
                "_link_loss_rate",
                link.link_stats.loss_rate,
                &[("link_id", link_id)],
                "Packet loss rate for this link",
            ));

            output.push_str(&self.format_prometheus_line(
                "_link_throughput_bps",
                link.link_stats.throughput_bps as f64,
                &[("link_id", link_id)],
                "Throughput for this link",
            ));

            output.push_str(&self.format_prometheus_line(
                "_link_queue_depth",
                link.queue_metrics.current_depth as f64,
                &[("link_id", link_id)],
                "Current queue depth for this link",
            ));

            output.push_str(&self.format_prometheus_line(
                "_link_queue_drops",
                link.queue_metrics.drop_count as f64,
                &[("link_id", link_id)],
                "Queue drops for this link",
            ));

            // Qdisc parameters
            if let Some(delay) = link.qdisc_params.delay_ms {
                output.push_str(&self.format_prometheus_line(
                    "_link_qdisc_delay_ms",
                    delay as f64,
                    &[
                        ("link_id", link_id),
                        ("qdisc", &link.qdisc_params.qdisc_type),
                    ],
                    "Configured delay for this link's qdisc",
                ));
            }

            if let Some(rate) = link.qdisc_params.rate_kbps {
                output.push_str(&self.format_prometheus_line(
                    "_link_qdisc_rate_kbps",
                    rate as f64,
                    &[
                        ("link_id", link_id),
                        ("qdisc", &link.qdisc_params.qdisc_type),
                    ],
                    "Configured rate for this link's qdisc",
                ));
            }

            if let Some(loss) = link.qdisc_params.loss_pct {
                output.push_str(&self.format_prometheus_line(
                    "_link_qdisc_loss_pct",
                    loss as f64,
                    &[
                        ("link_id", link_id),
                        ("qdisc", &link.qdisc_params.qdisc_type),
                    ],
                    "Configured loss percentage for this link's qdisc",
                ));
            }
        }

        Ok(output)
    }

    async fn export_to_file(&self, snapshot: &MetricsSnapshot, path: &Path) -> Result<()> {
        let content = self.export(snapshot).await?;
        let mut file = File::create(path).await?;
        file.write_all(content.as_bytes()).await?;
        Ok(())
    }
}

/// Exports metrics in JSON format
pub struct JsonExporter {
    pretty: bool,
}

impl JsonExporter {
    pub fn new() -> Self {
        Self { pretty: false }
    }

    pub fn pretty() -> Self {
        Self { pretty: true }
    }
}

#[async_trait]
impl MetricsExporter for JsonExporter {
    async fn export(&self, snapshot: &MetricsSnapshot) -> Result<String> {
        let result = if self.pretty {
            serde_json::to_string_pretty(snapshot)?
        } else {
            serde_json::to_string(snapshot)?
        };
        Ok(result)
    }

    async fn export_to_file(&self, snapshot: &MetricsSnapshot, path: &Path) -> Result<()> {
        let content = self.export(snapshot).await?;
        let mut file = File::create(path).await?;
        file.write_all(content.as_bytes()).await?;
        Ok(())
    }
}

/// Exports metrics in CSV format (one row per link)
pub struct CsvExporter {
    include_headers: bool,
}

impl CsvExporter {
    pub fn new() -> Self {
        Self {
            include_headers: true,
        }
    }

    pub fn without_headers() -> Self {
        Self {
            include_headers: false,
        }
    }
}

#[async_trait]
impl MetricsExporter for CsvExporter {
    async fn export(&self, snapshot: &MetricsSnapshot) -> Result<String> {
        let mut output = String::new();

        if self.include_headers {
            output.push_str("timestamp,simulation_id,link_id,bytes_sent,bytes_received,packets_sent,packets_received,packets_dropped,rtt_ms,jitter_ms,loss_rate,throughput_bps,queue_depth,queue_drops,qdisc_type,qdisc_delay_ms,qdisc_rate_kbps,qdisc_loss_pct\n");
        }

        for link in &snapshot.link_performance {
            let line = format!(
                "{},{},{},{},{},{},{},{},{:.2},{:.2},{:.4},{},{},{},{},{},{},{}\n",
                snapshot.timestamp.format("%Y-%m-%d %H:%M:%S%.3f"),
                snapshot.simulation_metrics.simulation_id,
                link.link_stats.link_id,
                link.link_stats.bytes_sent,
                link.link_stats.bytes_received,
                link.link_stats.packets_sent,
                link.link_stats.packets_received,
                link.link_stats.packets_dropped,
                link.link_stats.rtt_ms,
                link.link_stats.jitter_ms,
                link.link_stats.loss_rate,
                link.link_stats.throughput_bps,
                link.queue_metrics.current_depth,
                link.queue_metrics.drop_count,
                link.qdisc_params.qdisc_type,
                link.qdisc_params
                    .delay_ms
                    .map_or("".to_string(), |d| d.to_string()),
                link.qdisc_params
                    .rate_kbps
                    .map_or("".to_string(), |r| r.to_string()),
                link.qdisc_params
                    .loss_pct
                    .map_or("".to_string(), |l| l.to_string()),
            );
            output.push_str(&line);
        }

        Ok(output)
    }

    async fn export_to_file(&self, snapshot: &MetricsSnapshot, path: &Path) -> Result<()> {
        let content = self.export(snapshot).await?;
        let mut file = File::create(path).await?;
        file.write_all(content.as_bytes()).await?;
        Ok(())
    }
}
