//! Trace recording and replay functionality

use crate::{MetricsSnapshot, ObservabilityError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::{AsyncWriteExt, BufWriter};

/// A single entry in a trace recording
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEntry {
    pub timestamp: DateTime<Utc>,
    pub event_type: String,
    pub link_id: Option<String>,
    pub data: serde_json::Value,
}

impl TraceEntry {
    pub fn new(event_type: String, data: serde_json::Value) -> Self {
        Self {
            timestamp: Utc::now(),
            event_type,
            link_id: None,
            data,
        }
    }

    pub fn with_link_id(mut self, link_id: String) -> Self {
        self.link_id = Some(link_id);
        self
    }
}

/// Schedule for replaying trace entries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplaySchedule {
    pub entries: Vec<TraceEntry>,
    pub start_time: DateTime<Utc>,
    pub time_scale_factor: f64, // 1.0 = real-time, 2.0 = 2x speed, etc.
}

impl ReplaySchedule {
    pub fn new(entries: Vec<TraceEntry>) -> Self {
        Self {
            start_time: entries
                .first()
                .map(|e| e.timestamp)
                .unwrap_or_else(Utc::now),
            entries,
            time_scale_factor: 1.0,
        }
    }

    pub fn with_time_scale(mut self, factor: f64) -> Self {
        self.time_scale_factor = factor;
        self
    }
}

/// Records trace entries to disk for later replay
pub struct TraceRecorder {
    file_path: PathBuf,
    writer: Option<BufWriter<File>>,
    entries_recorded: u64,
}

impl TraceRecorder {
    pub fn new(path: &str) -> Result<Self> {
        Ok(Self {
            file_path: PathBuf::from(path),
            writer: None,
            entries_recorded: 0,
        })
    }

    /// Initialize the trace file for writing
    pub async fn initialize(&mut self) -> Result<()> {
        let file = File::create(&self.file_path).await?;
        self.writer = Some(BufWriter::new(file));
        Ok(())
    }

    /// Record a trace entry
    pub async fn record(&mut self, entry: TraceEntry) -> Result<()> {
        if self.writer.is_none() {
            self.initialize().await?;
        }

        if let Some(ref mut writer) = self.writer {
            let json = serde_json::to_string(&entry)?;
            writer.write_all(json.as_bytes()).await?;
            writer.write_all(b"\n").await?;
            writer.flush().await?;
            self.entries_recorded += 1;
        }

        Ok(())
    }

    /// Record a metrics snapshot as a trace entry
    pub async fn record_snapshot(&mut self, snapshot: &MetricsSnapshot) -> Result<()> {
        let entry = TraceEntry::new(
            "metrics_snapshot".to_string(),
            serde_json::to_value(snapshot)?,
        );
        self.record(entry).await
    }

    /// Get the number of entries recorded
    pub fn entries_recorded(&self) -> u64 {
        self.entries_recorded
    }

    /// Close the trace file
    pub async fn close(&mut self) -> Result<()> {
        if let Some(mut writer) = self.writer.take() {
            writer.flush().await?;
        }
        Ok(())
    }
}

/// Replays trace entries from a recorded file
pub struct TraceReplay {
    file_path: PathBuf,
    schedule: Option<ReplaySchedule>,
}

impl TraceReplay {
    pub fn new(path: &str) -> Self {
        Self {
            file_path: PathBuf::from(path),
            schedule: None,
        }
    }

    /// Load trace entries from file
    pub async fn load(&mut self) -> Result<()> {
        let content = tokio::fs::read_to_string(&self.file_path).await?;
        let mut entries = Vec::new();

        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let entry: TraceEntry = serde_json::from_str(line)?;
            entries.push(entry);
        }

        self.schedule = Some(ReplaySchedule::new(entries));
        Ok(())
    }

    /// Get the replay schedule
    pub fn get_schedule(&self) -> Option<&ReplaySchedule> {
        self.schedule.as_ref()
    }

    /// Start replaying entries with a callback
    pub async fn replay<F>(&self, mut callback: F) -> Result<()>
    where
        F: FnMut(&TraceEntry) -> Result<()>,
    {
        let schedule = self
            .schedule
            .as_ref()
            .ok_or_else(|| ObservabilityError::Trace("Schedule not loaded".to_string()))?;

        let start = tokio::time::Instant::now();

        for entry in &schedule.entries {
            let elapsed =
                (entry.timestamp - schedule.start_time).num_milliseconds() as f64 / 1000.0;
            let scaled_elapsed = elapsed / schedule.time_scale_factor;

            let target = start + tokio::time::Duration::from_secs_f64(scaled_elapsed);
            tokio::time::sleep_until(target).await;

            callback(entry)?;
        }

        Ok(())
    }
}

impl Drop for TraceRecorder {
    fn drop(&mut self) {
        if let Some(mut writer) = self.writer.take() {
            // Best effort flush on drop
            let _ = futures::executor::block_on(writer.flush());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        LinkPerformance, LinkStats, MetricsSnapshot, QdiscParams, QueueMetrics, SimulationMetrics,
    };
    use tempfile::NamedTempFile;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_trace_entry_creation() {
        let data = serde_json::json!({
            "test": "value",
            "number": 42
        });

        let entry = TraceEntry::new("test_event".to_string(), data.clone());

        assert_eq!(entry.event_type, "test_event");
        assert_eq!(entry.data, data);
        assert!(entry.link_id.is_none());
        assert!(entry.timestamp <= Utc::now());
    }

    #[tokio::test]
    async fn test_trace_entry_with_link_id() {
        let data = serde_json::json!({
            "metric": "throughput",
            "value": 1000000
        });

        let entry = TraceEntry::new("throughput_update".to_string(), data.clone())
            .with_link_id("link_123".to_string());

        assert_eq!(entry.event_type, "throughput_update");
        assert_eq!(entry.data, data);
        assert_eq!(entry.link_id, Some("link_123".to_string()));
    }

    #[tokio::test]
    async fn test_replay_schedule_creation() {
        let entries = vec![
            TraceEntry::new("event1".to_string(), serde_json::json!({"value": 1})),
            TraceEntry::new("event2".to_string(), serde_json::json!({"value": 2})),
        ];
        let start_time = entries[0].timestamp;

        let schedule = ReplaySchedule::new(entries.clone());

        assert_eq!(schedule.entries.len(), 2);
        assert_eq!(schedule.start_time, start_time);
        assert_eq!(schedule.time_scale_factor, 1.0);
    }

    #[tokio::test]
    async fn test_replay_schedule_with_time_scale() {
        let entries = vec![TraceEntry::new(
            "event1".to_string(),
            serde_json::json!({"value": 1}),
        )];

        let schedule = ReplaySchedule::new(entries).with_time_scale(2.0);

        assert_eq!(schedule.time_scale_factor, 2.0);
    }

    #[tokio::test]
    async fn test_trace_recorder_creation() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();

        let recorder = TraceRecorder::new(path).unwrap();

        assert_eq!(recorder.file_path, PathBuf::from(path));
        assert!(recorder.writer.is_none());
        assert_eq!(recorder.entries_recorded, 0);
    }

    #[tokio::test]
    async fn test_trace_recorder_initialization() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();

        let mut recorder = TraceRecorder::new(path).unwrap();
        recorder.initialize().await.unwrap();

        assert!(recorder.writer.is_some());
        assert_eq!(recorder.entries_recorded, 0);
    }

    #[tokio::test]
    async fn test_trace_recorder_record_entry() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();

        let mut recorder = TraceRecorder::new(path).unwrap();

        let entry = TraceEntry::new(
            "test_event".to_string(),
            serde_json::json!({"test": "data", "value": 123}),
        );

        recorder.record(entry).await.unwrap();

        assert_eq!(recorder.entries_recorded(), 1);

        // Close the recorder to flush
        recorder.close().await.unwrap();

        // Verify the file contains the entry
        let content = tokio::fs::read_to_string(path).await.unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 1);

        let recorded_entry: TraceEntry = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(recorded_entry.event_type, "test_event");
    }

    #[tokio::test]
    async fn test_trace_recorder_record_multiple_entries() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();

        let mut recorder = TraceRecorder::new(path).unwrap();

        let entries = vec![
            TraceEntry::new("event1".to_string(), serde_json::json!({"value": 1})),
            TraceEntry::new("event2".to_string(), serde_json::json!({"value": 2}))
                .with_link_id("link1".to_string()),
            TraceEntry::new("event3".to_string(), serde_json::json!({"value": 3})),
        ];

        for entry in entries {
            recorder.record(entry).await.unwrap();
        }

        assert_eq!(recorder.entries_recorded(), 3);

        recorder.close().await.unwrap();

        // Verify all entries were recorded
        let content = tokio::fs::read_to_string(path).await.unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 3);

        let entry2: TraceEntry = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(entry2.event_type, "event2");
        assert_eq!(entry2.link_id, Some("link1".to_string()));
    }

    #[tokio::test]
    async fn test_trace_recorder_record_snapshot() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();

        let mut recorder = TraceRecorder::new(path).unwrap();

        // Create a test metrics snapshot
        let snapshot = create_test_snapshot();

        recorder.record_snapshot(&snapshot).await.unwrap();

        assert_eq!(recorder.entries_recorded(), 1);

        recorder.close().await.unwrap();

        // Verify the snapshot was recorded
        let content = tokio::fs::read_to_string(path).await.unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 1);

        let recorded_entry: TraceEntry = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(recorded_entry.event_type, "metrics_snapshot");
    }

    #[tokio::test]
    async fn test_trace_recorder_auto_initialization() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();

        let mut recorder = TraceRecorder::new(path).unwrap();

        // Record without explicit initialization should auto-initialize
        let entry = TraceEntry::new("auto_init_test".to_string(), serde_json::json!({}));
        recorder.record(entry).await.unwrap();

        assert!(recorder.writer.is_some());
        assert_eq!(recorder.entries_recorded(), 1);
    }

    #[tokio::test]
    async fn test_trace_replay_creation() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();

        let replay = TraceReplay::new(path);

        assert_eq!(replay.file_path, PathBuf::from(path));
        assert!(replay.schedule.is_none());
    }

    #[tokio::test]
    async fn test_trace_replay_load_entries() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();

        // Write test entries to file
        let entries = vec![
            TraceEntry::new("event1".to_string(), serde_json::json!({"value": 1})),
            TraceEntry::new("event2".to_string(), serde_json::json!({"value": 2})),
        ];

        let mut content = String::new();
        for entry in &entries {
            content.push_str(&serde_json::to_string(entry).unwrap());
            content.push('\n');
        }

        tokio::fs::write(path, content).await.unwrap();

        // Load and verify
        let mut replay = TraceReplay::new(path);
        replay.load().await.unwrap();

        let schedule = replay.get_schedule().unwrap();
        assert_eq!(schedule.entries.len(), 2);
        assert_eq!(schedule.entries[0].event_type, "event1");
        assert_eq!(schedule.entries[1].event_type, "event2");
    }

    #[tokio::test]
    async fn test_trace_replay_load_empty_file() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();

        // Write empty file
        tokio::fs::write(path, "").await.unwrap();

        let mut replay = TraceReplay::new(path);
        replay.load().await.unwrap();

        let schedule = replay.get_schedule().unwrap();
        assert_eq!(schedule.entries.len(), 0);
    }

    #[tokio::test]
    async fn test_trace_replay_load_with_empty_lines() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();

        // Write file with empty lines
        let entry = TraceEntry::new("test".to_string(), serde_json::json!({}));
        let content = format!("\n{}\n\n", serde_json::to_string(&entry).unwrap());

        tokio::fs::write(path, content).await.unwrap();

        let mut replay = TraceReplay::new(path);
        replay.load().await.unwrap();

        let schedule = replay.get_schedule().unwrap();
        assert_eq!(schedule.entries.len(), 1);
        assert_eq!(schedule.entries[0].event_type, "test");
    }

    #[tokio::test]
    async fn test_trace_replay_callback() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();

        // Create entries with small time intervals for fast test
        let now = Utc::now();
        let entries = vec![
            TraceEntry {
                timestamp: now,
                event_type: "event1".to_string(),
                link_id: None,
                data: serde_json::json!({"value": 1}),
            },
            TraceEntry {
                timestamp: now + chrono::Duration::milliseconds(100),
                event_type: "event2".to_string(),
                link_id: None,
                data: serde_json::json!({"value": 2}),
            },
        ];

        let mut content = String::new();
        for entry in &entries {
            content.push_str(&serde_json::to_string(entry).unwrap());
            content.push('\n');
        }

        tokio::fs::write(path, content).await.unwrap();

        let mut replay = TraceReplay::new(path);
        replay.load().await.unwrap();

        // Replay with fast time scale
        let schedule = replay.schedule.as_mut().unwrap();
        schedule.time_scale_factor = 100.0; // 100x speed

        let mut received_entries = Vec::new();

        let start = tokio::time::Instant::now();
        replay
            .replay(|entry| {
                received_entries.push(entry.clone());
                Ok(())
            })
            .await
            .unwrap();
        let elapsed = start.elapsed();

        assert_eq!(received_entries.len(), 2);
        assert_eq!(received_entries[0].event_type, "event1");
        assert_eq!(received_entries[1].event_type, "event2");

        // Should complete quickly due to time scaling
        assert!(elapsed.as_millis() < 50); // Much less than 100ms due to 100x speed
    }

    #[tokio::test]
    async fn test_trace_replay_error_handling() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();

        let replay = TraceReplay::new(path);

        // Should fail to replay without loading
        let result = replay.replay(|_| Ok(())).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Schedule not loaded"));
    }

    // Helper function to create a test metrics snapshot
    fn create_test_snapshot() -> MetricsSnapshot {
        let link_stats = LinkStats::new("test_link".to_string());
        let qdisc_params = QdiscParams::new("netem".to_string());
        let queue_metrics = QueueMetrics::new();

        let link_performance = LinkPerformance {
            link_stats,
            qdisc_params,
            queue_metrics,
        };

        let simulation_metrics = SimulationMetrics {
            simulation_id: Uuid::new_v4(),
            start_time: Utc::now(),
            duration_ms: 1000,
            total_bytes_sent: 1000,
            total_bytes_received: 2000,
            total_packets_sent: 10,
            total_packets_received: 20,
            total_drops: 1,
            avg_rtt_ms: 25.0,
            avg_jitter_ms: 5.0,
            avg_loss_rate: 0.1,
            total_throughput_bps: 500_000,
            active_links: 1,
            link_ids: vec!["test_link".to_string()],
        };

        MetricsSnapshot {
            timestamp: Utc::now(),
            simulation_metrics,
            link_performance: vec![link_performance],
            metadata: serde_json::json!({"test": true}),
        }
    }
}
