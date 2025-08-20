//! Observability and metrics collection for RIST network simulation
//!
//! This crate provides comprehensive monitoring, metrics collection, and trace recording
//! for network simulations and RIST bonding scenarios. It's designed to integrate with
//! both netlink-sim and netns-testbench backends while providing unified observability.
//!
//! # Features
//!
//! - **Metrics Collection**: Prometheus-compatible metrics for link performance, queue depths, and throughput
//! - **Trace Recording**: Time-series data export to CSV/JSON for replay and analysis
//! - **GStreamer Integration**: Bus message correlation with simulation metrics
//! - **Real-time Monitoring**: Live dashboards and alerting for network conditions
//! - **Test Integration**: Automated assertions on metrics during integration tests

pub mod collector;
pub mod exporter;
pub mod gstreamer;
pub mod metrics;
pub mod recorder;
pub mod server;

pub use collector::{LinkMetricsCollector, LinkPerformance, MetricsCollector, QueueMetrics};
pub use exporter::{CsvExporter, JsonExporter, MetricsExporter, PrometheusExporter};
pub use gstreamer::{BusMessageFilter, GstBusCollector, RistDispatcherMetrics};
pub use metrics::{LinkStats, MetricsSnapshot, QdiscParams, SimulationMetrics};
pub use recorder::{ReplaySchedule, TraceEntry, TraceRecorder, TraceReplay};
pub use server::{MetricsServer, ObservabilityConfig};

/// Errors that can occur in the observability system
#[derive(thiserror::Error, Debug)]
pub enum ObservabilityError {
    #[error("Metrics collection failed: {0}")]
    Collection(String),

    #[error("Export failed: {0}")]
    Export(#[from] anyhow::Error),

    #[error("Server error: {0}")]
    Server(String),

    #[error("Trace recording error: {0}")]
    Trace(String),

    #[error("GStreamer bus error: {0}")]
    GStreamer(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Result type for observability operations
pub type Result<T> = std::result::Result<T, ObservabilityError>;

/// Main observability orchestrator that coordinates all monitoring subsystems
pub struct ObservabilityOrchestrator {
    collector: MetricsCollector,
    exporter: Box<dyn MetricsExporter + Send + Sync>,
    recorder: Option<TraceRecorder>,
    gst_collector: Option<GstBusCollector>,
    server: Option<MetricsServer>,
}

impl ObservabilityOrchestrator {
    /// Create a new observability orchestrator with default configuration
    pub fn new() -> Result<Self> {
        let collector = MetricsCollector::new();
        let exporter = Box::new(PrometheusExporter::new()?);

        Ok(Self {
            collector,
            exporter,
            recorder: None,
            gst_collector: None,
            server: None,
        })
    }

    /// Enable trace recording to the specified path
    pub fn with_trace_recording(mut self, path: &str) -> Result<Self> {
        self.recorder = Some(TraceRecorder::new(path)?);
        Ok(self)
    }

    /// Enable GStreamer bus monitoring
    pub fn with_gstreamer_monitoring(mut self) -> Result<Self> {
        self.gst_collector = Some(GstBusCollector::new()?);
        Ok(self)
    }

    /// Start HTTP server for metrics endpoint
    pub async fn start_server(mut self, config: ObservabilityConfig) -> Result<Self> {
        let server = MetricsServer::new(config);
        server.start().await?;
        self.server = Some(server);
        Ok(self)
    }

    /// Get current metrics snapshot
    pub async fn get_snapshot(&self) -> Result<MetricsSnapshot> {
        self.collector.take_snapshot().await
    }

    /// Record a trace entry if recording is enabled
    pub async fn record_trace(&mut self, entry: TraceEntry) -> Result<()> {
        if let Some(ref mut recorder) = self.recorder {
            recorder.record(entry).await?;
        }
        Ok(())
    }

    /// Export current metrics using configured exporter
    pub async fn export_metrics(&self) -> Result<String> {
        let snapshot = self.get_snapshot().await?;
        self.exporter.export(&snapshot).await
    }
}

impl Default for ObservabilityOrchestrator {
    fn default() -> Self {
        Self::new().expect("Failed to create default observability orchestrator")
    }
}
