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
