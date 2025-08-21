use observability::{
    CsvExporter, JsonExporter, MetricsCollector, MetricsExporter, ObservabilityConfig,
    ObservabilityOrchestrator, PrometheusExporter, QdiscParams, TraceEntry,
};
use scenarios::DirectionSpec;
use serde_json::json;
use std::time::Duration;
use tokio::time::sleep;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    info!("Starting RIST Observability & Metrics Demo");

    // Create observability orchestrator with full configuration
    let config = ObservabilityConfig {
        bind_address: "127.0.0.1".to_string(),
        port: 8080,
        enable_cors: true,
        dashboard_enabled: true,
    };

    let mut orchestrator = ObservabilityOrchestrator::new()?
        .with_trace_recording("/tmp/rist-simulation.trace")?
        .with_gstreamer_monitoring()?
        .start_server(config)
        .await?;

    // Create metrics collector for race car scenario
    let collector = MetricsCollector::new();

    // Add 4G and 5G race car links
    let link_4g = collector.add_link("race_4g_link_1".to_string());
    let link_5g = collector.add_link("race_5g_link_1".to_string());

    info!("📊 Setting up race car cellular monitoring:");
    info!("- Link 1: 4G race conditions (300-2000 kbps)");
    info!("- Link 2: 5G race conditions (400-2000 kbps)");
    info!("- Trace recording enabled");
    info!("- Dashboard available at http://127.0.0.1:8080/");

    // Simulate race car conditions over time
    for i in 0..10 {
        let elapsed = i as f64;

        // Simulate race car 4G link conditions
        let race_4g_spec = if i < 3 {
            DirectionSpec::race_4g_strong()
        } else if i < 7 {
            DirectionSpec::race_4g_moderate()
        } else {
            DirectionSpec::race_4g_weak()
        };

        // Simulate race car 5G link conditions
        let race_5g_spec = if i < 2 {
            DirectionSpec::race_5g_strong()
        } else if i < 8 {
            DirectionSpec::race_5g_moderate()
        } else {
            DirectionSpec::race_5g_weak()
        };

        // Update 4G link metrics (simulated)
        link_4g.record_traffic(
            500_000 + i * 100_000, // bytes sent
            400_000 + i * 80_000,  // bytes received
            5000 + i * 1000,       // packets sent
            4500 + i * 900,        // packets received
            50 + i * 10,           // packets dropped
        );

        link_4g.update_quality(
            race_4g_spec.base_delay_ms as f64,
            race_4g_spec.jitter_ms as f64,
            race_4g_spec.loss_pct as f64,
            (race_4g_spec.rate_kbps * 1000) as u64, // Convert to bps
        );

        let qdisc_4g = QdiscParams {
            qdisc_type: "netem_tbf".to_string(),
            delay_ms: Some(race_4g_spec.base_delay_ms as u64),
            jitter_ms: Some(race_4g_spec.jitter_ms as u64),
            loss_pct: Some(race_4g_spec.loss_pct),
            rate_kbps: Some(race_4g_spec.rate_kbps as u64),
            burst_bytes: Some(32 * 1024),
            last_changed: chrono::Utc::now(),
        };
        link_4g.update_qdisc(qdisc_4g);

        // Update 5G link metrics (simulated)
        link_5g.record_traffic(
            800_000 + i * 150_000, // bytes sent
            700_000 + i * 120_000, // bytes received
            8000 + i * 1500,       // packets sent
            7200 + i * 1300,       // packets received
            30 + i * 5,            // packets dropped
        );

        link_5g.update_quality(
            race_5g_spec.base_delay_ms as f64,
            race_5g_spec.jitter_ms as f64,
            race_5g_spec.loss_pct as f64,
            (race_5g_spec.rate_kbps * 1000) as u64, // Convert to bps
        );

        let qdisc_5g = QdiscParams {
            qdisc_type: "netem_tbf".to_string(),
            delay_ms: Some(race_5g_spec.base_delay_ms as u64),
            jitter_ms: Some(race_5g_spec.jitter_ms as u64),
            loss_pct: Some(race_5g_spec.loss_pct),
            rate_kbps: Some(race_5g_spec.rate_kbps as u64),
            burst_bytes: Some(64 * 1024),
            last_changed: chrono::Utc::now(),
        };
        link_5g.update_qdisc(qdisc_5g);

        // Take metrics snapshot
        let snapshot = collector.take_snapshot().await?;

        info!(
            "📈 T+{}s: 4G={:.0}kbps {:.1}ms {:.2}% | 5G={:.0}kbps {:.1}ms {:.2}%",
            elapsed,
            snapshot.link_performance[0].link_stats.throughput_bps as f64 / 1000.0,
            snapshot.link_performance[0].link_stats.rtt_ms,
            snapshot.link_performance[0].link_stats.loss_rate * 100.0,
            snapshot.link_performance[1].link_stats.throughput_bps as f64 / 1000.0,
            snapshot.link_performance[1].link_stats.rtt_ms,
            snapshot.link_performance[1].link_stats.loss_rate * 100.0,
        );

        // Record trace entry
        let trace_entry = TraceEntry::new(
            "link_conditions_update".to_string(),
            json!({
                "elapsed_seconds": elapsed,
                "4g_throughput_bps": snapshot.link_performance[0].link_stats.throughput_bps,
                "4g_rtt_ms": snapshot.link_performance[0].link_stats.rtt_ms,
                "4g_loss_rate": snapshot.link_performance[0].link_stats.loss_rate,
                "5g_throughput_bps": snapshot.link_performance[1].link_stats.throughput_bps,
                "5g_rtt_ms": snapshot.link_performance[1].link_stats.rtt_ms,
                "5g_loss_rate": snapshot.link_performance[1].link_stats.loss_rate,
            }),
        );
        orchestrator.record_trace(trace_entry).await?;

        sleep(Duration::from_secs(1)).await;
    }

    // Export final metrics in different formats
    let final_snapshot = collector.take_snapshot().await?;

    info!("📋 Exporting final metrics in multiple formats:");

    // Prometheus format
    let prometheus_exporter = PrometheusExporter::new()?;
    let prometheus_metrics = prometheus_exporter.export(&final_snapshot).await?;
    info!(
        "✅ Prometheus format: {} lines",
        prometheus_metrics.lines().count()
    );

    // JSON format
    let json_exporter = JsonExporter::pretty();
    let json_metrics = json_exporter.export(&final_snapshot).await?;
    info!("✅ JSON format: {} characters", json_metrics.len());

    // CSV format
    let csv_exporter = CsvExporter::new();
    let csv_metrics = csv_exporter.export(&final_snapshot).await?;
    info!("✅ CSV format: {} rows", csv_metrics.lines().count());

    // Export to files
    prometheus_exporter
        .export_to_file(&final_snapshot, "/tmp/rist-metrics.prom".as_ref())
        .await?;
    json_exporter
        .export_to_file(&final_snapshot, "/tmp/rist-metrics.json".as_ref())
        .await?;
    csv_exporter
        .export_to_file(&final_snapshot, "/tmp/rist-metrics.csv".as_ref())
        .await?;

    info!("💾 Metrics exported to:");
    info!("- /tmp/rist-metrics.prom (Prometheus)");
    info!("- /tmp/rist-metrics.json (JSON)");
    info!("- /tmp/rist-metrics.csv (CSV)");
    info!("- /tmp/rist-simulation.trace (Trace replay)");

    // Final summary
    info!("🏁 Race car observability demo completed:");
    info!(
        "- {} active links monitored",
        final_snapshot.simulation_metrics.active_links
    );
    info!(
        "- {:.1} Mbps total throughput",
        final_snapshot.simulation_metrics.total_throughput_bps as f64 / 1_000_000.0
    );
    info!(
        "- {:.1}ms average RTT",
        final_snapshot.simulation_metrics.avg_rtt_ms
    );
    info!(
        "- {:.2}% average loss rate",
        final_snapshot.simulation_metrics.avg_loss_rate * 100.0
    );

    info!("🌐 Dashboard running at http://127.0.0.1:8080/ (Ctrl+C to stop)");

    // Keep the server running
    sleep(Duration::from_secs(30)).await;

    Ok(())
}
