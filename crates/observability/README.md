# observability

Observability and metrics collection for RIST network simulation.

## Overview

This crate provides comprehensive monitoring, metrics collection, and trace recording for network simulations and RIST bonding scenarios. It's designed to integrate with the `netns-testbench` backend while providing unified observability across the entire testing ecosystem.

## Features

- **Metrics Collection**: Prometheus-compatible metrics for link performance, queue depths, and throughput
- **Trace Recording**: Time-series data export to CSV/JSON for replay and analysis
- **GStreamer Integration**: Bus message correlation with simulation metrics
- **Real-time Monitoring**: Live dashboards and alerting for network conditions
- **Test Integration**: Automated assertions on metrics during integration tests
- **Backend Support**: Works with the netns-testbench backend

## Key Components

### Metrics Collection
- `MetricsCollector`: Main collector for simulation metrics
- `LinkMetricsCollector`: Specialized collector for network link performance
- `QueueMetrics`: Queue depth and packet statistics
- `LinkPerformance`: Throughput, latency, and loss measurements

### Data Export
- `PrometheusExporter`: Export metrics to Prometheus format
- `CsvExporter`: Export time-series data to CSV files
- `JsonExporter`: Structured JSON export for analysis tools
- `TraceRecorder`: Record and replay network trace data

### GStreamer Integration
- `GstBusCollector`: Capture GStreamer bus messages
- `RistDispatcherMetrics`: RIST-specific element metrics
- `BusMessageFilter`: Filter and correlate GStreamer events

### Monitoring Server
- `MetricsServer`: HTTP server for Prometheus scraping
- `ObservabilityConfig`: Configuration for monitoring setup

## Usage

### Basic Metrics Collection

```rust
use observability::{MetricsCollector, PrometheusExporter};

// Create a metrics collector
let mut collector = MetricsCollector::new();

// Record metrics during simulation
collector.record_link_performance("link1", 
    LinkPerformance {
        throughput_bps: 1_000_000,
        latency_ms: 50.0,
        packet_loss_percent: 1.5,
        timestamp: std::time::Instant::now(),
    }
);

// Export to Prometheus format
let exporter = PrometheusExporter::new();
let metrics_data = exporter.export(&collector)?;
```

### Trace Recording and Replay

```rust
use observability::{TraceRecorder, CsvExporter};

let mut recorder = TraceRecorder::new();

// Record network events during test
recorder.record_event("packet_sent", serde_json::json!({
    "timestamp": 1234567890,
    "size": 1500,
    "link": "uplink"
}));

// Export trace for analysis
let exporter = CsvExporter::new("network_trace.csv");
exporter.export_trace(&recorder)?;
```

### GStreamer Integration

```rust
use observability::{GstBusCollector, RistDispatcherMetrics};

// Monitor RIST dispatcher performance
let bus_collector = GstBusCollector::new(pipeline.bus()?);
let rist_metrics = RistDispatcherMetrics::new("ristdispatcher0");

// Collect metrics during pipeline execution
bus_collector.start_collection();
// ... run pipeline ...
let performance_data = rist_metrics.collect_metrics();
```

### Monitoring Server Setup

```rust
use observability::{MetricsServer, ObservabilityConfig};

let config = ObservabilityConfig {
    prometheus_port: 9090,
    enable_trace_recording: true,
    csv_output_dir: "/tmp/traces".into(),
};

let server = MetricsServer::new(config);
server.start().await?;
// Metrics available at http://localhost:9090/metrics
```

## Monitoring Capabilities

### Network Link Metrics
- Throughput (bps)
- Latency (round-trip and one-way)
- Packet loss percentage
- Jitter and delay variation
- Queue utilization and depth

### RIST-Specific Metrics
- Bond utilization across links
- Retransmission rates
- Buffer underruns/overruns
- Link failover events
- Bitrate adaptation decisions

### System Metrics
- CPU and memory usage
- Network interface statistics
- GStreamer element performance
- Test scenario execution time

## Integration with Testing

The observability system integrates seamlessly with:
- `integration_tests`: Automated metric assertions
- `netns-testbench`: Network namespace monitoring
- `netns-testbench`: Network namespace backend instrumentation
- `rist-elements`: GStreamer element metrics

## Output Formats

- **Prometheus**: Real-time metrics scraping
- **CSV**: Time-series data for spreadsheet analysis
- **JSON**: Structured data for custom analysis tools
- **InfluxDB**: Time-series database integration (planned)
- **Grafana**: Dashboard visualization (via Prometheus)