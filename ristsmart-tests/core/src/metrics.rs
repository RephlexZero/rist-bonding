use anyhow::{bail, Result};
use gstreamer as gst;
use serde::{Serialize, Deserialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RistSessionStats {
    pub original_packets: u64,
    pub retrans_packets: u64,
    pub rtt_ms: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RistStatsSnapshot {
    pub dyn_bitrate_kbps: Option<u32>,
    pub dispatcher_weights: Option<Vec<f64>>,
    pub rist_sessions: Option<Vec<RistSessionStats>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Sample {
    pub ts_ms: u64,
    pub achieved_bps: f64,
    pub theoretical_bps: f64,
    pub rx_bytes_total: u64,
    pub link_bytes: Vec<u64>,
    pub capacities_mbps: Vec<f64>,
    pub loss_rate: Vec<f64>,
    pub delay_ms: Vec<u64>,
    pub dyn_bitrate_kbps: Option<u32>,
    pub dispatcher_weights: Option<Vec<f64>>,
    pub ideal_weights: Option<Vec<f64>>,
    pub sessions: Option<Vec<RistSessionStats>>,
}

#[derive(Default, Serialize, Deserialize, Clone)]
pub struct Metrics {
    pub samples: Vec<Sample>,
}

impl Metrics {
    pub fn record(&mut self, s: Sample) { 
        self.samples.push(s); 
    }

    pub fn save_json(&self, path: &std::path::Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    pub fn save_csv(&self, path: &std::path::Path) -> Result<()> {
        let mut wtr = csv::Writer::from_path(path)?;
        
        // Write header
        wtr.write_record(&[
            "ts_ms", "achieved_bps", "theoretical_bps", "rx_bytes_total",
            "link0_bytes", "link1_bytes", // Extend for more links as needed
            "cap0_mbps", "cap1_mbps",
            "loss0", "loss1", 
            "delay0_ms", "delay1_ms",
            "dyn_bitrate_kbps",
            "disp_weight0", "disp_weight1",
            "ideal_weight0", "ideal_weight1",
        ])?;

        // Write data rows
        for sample in &self.samples {
            let mut record = vec![
                sample.ts_ms.to_string(),
                sample.achieved_bps.to_string(),
                sample.theoretical_bps.to_string(),
                sample.rx_bytes_total.to_string(),
            ];

            // Link bytes (pad to at least 2 links)
            for i in 0..2 {
                record.push(sample.link_bytes.get(i).map(|v| v.to_string()).unwrap_or_default());
            }

            // Capacities (pad to at least 2 links)
            for i in 0..2 {
                record.push(sample.capacities_mbps.get(i).map(|v| v.to_string()).unwrap_or_default());
            }

            // Loss rates
            for i in 0..2 {
                record.push(sample.loss_rate.get(i).map(|v| v.to_string()).unwrap_or_default());
            }

            // Delays
            for i in 0..2 {
                record.push(sample.delay_ms.get(i).map(|v| v.to_string()).unwrap_or_default());
            }

            // Dynamic bitrate
            record.push(sample.dyn_bitrate_kbps.map(|v| v.to_string()).unwrap_or_default());

            // Dispatcher weights
            for i in 0..2 {
                let weight = sample.dispatcher_weights.as_ref()
                    .and_then(|w| w.get(i))
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                record.push(weight);
            }

            // Ideal weights  
            for i in 0..2 {
                let weight = sample.ideal_weights.as_ref()
                    .and_then(|w| w.get(i))
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                record.push(weight);
            }

            wtr.write_record(&record)?;
        }

        wtr.flush()?;
        Ok(())
    }
}

#[derive(Default)]
pub struct SamplesContext {
    pub metrics: Metrics,

    pub last_dyn_bitrate_kbps: Option<u32>,
    pub last_dispatcher_weights: Option<Vec<f64>>,
    pub last_rist_sessions: Option<Vec<RistSessionStats>>,
}

impl SamplesContext {
    pub fn into_context(self, run_id: String, outdir: PathBuf, links: usize, scenario: crate::scenarios::ScenarioKind, efficiency: f64) -> Result<RunContext> {
        Ok(RunContext { 
            run_id, 
            outdir, 
            links, 
            scenario, 
            efficiency, 
            metrics: self.metrics 
        })
    }
}

pub fn parse_rist_stats(_st: &gst::Structure) -> Result<Vec<RistSessionStats>> {
    // Parse gst::Structure produced by ristsink/ristsrc stats; adjust field names as needed.
    // For example, st might contain an array/list of session structures.
    // TODO: This needs to be implemented based on the actual RIST stats structure in gst-plugins-bad
    bail!("Implement parse_rist_stats based on the actual RIST stats structure in gst-plugins-bad");
}

#[derive(Clone, Debug)]
pub struct DispatcherWeights(pub Vec<f64>);

#[derive(Clone, Debug)]
pub struct DynBitrate(pub u32);

#[derive(Default)]
pub struct KpiPolicy {
    pub min_share_on_fat_link: Option<f64>, // e.g., 0.7
    pub max_failover_time_s: Option<f64>,   // e.g., 2.0
    pub min_throughput_ratio: Option<f64>,  // achieved/theoretical >= 0.85
}

impl KpiPolicy {
    pub fn defaults_for(s: crate::scenarios::ScenarioKind) -> Self {
        use crate::scenarios::ScenarioKind;
        match s {
            ScenarioKind::Baseline => KpiPolicy {
                min_throughput_ratio: Some(0.90), 
                ..Default::default()
            },
            ScenarioKind::AsymmetricBw => KpiPolicy {
                min_throughput_ratio: Some(0.85), 
                min_share_on_fat_link: Some(0.70), 
                ..Default::default()
            },
            ScenarioKind::Blackhole => KpiPolicy {
                min_throughput_ratio: Some(0.70), 
                max_failover_time_s: Some(2.0), 
                ..Default::default()
            },
            _ => KpiPolicy { 
                min_throughput_ratio: Some(0.80), 
                ..Default::default() 
            },
        }
    }

    pub fn observe(&mut self, _scenario: &str, _probes: &crate::pipelines::SampleProbes) -> Result<()> {
        // Implement per-tick checks if desired
        Ok(())
    }

    pub fn assert_all(&self, ctx: &RunContext) -> Result<()> {
        // Compute final KPIs and enforce thresholds
        if let Some(min_ratio) = self.min_throughput_ratio {
            let ratios: Vec<f64> = ctx.metrics.samples.iter()
                .filter(|s| s.theoretical_bps > 0.0)
                .map(|s| s.achieved_bps / s.theoretical_bps)
                .collect();
            
            if !ratios.is_empty() {
                let mean_ratio = ratios.iter().sum::<f64>() / ratios.len() as f64;
                if mean_ratio < min_ratio {
                    bail!("Mean throughput ratio {:.3} < required {:.3}", mean_ratio, min_ratio);
                }
            }
        }

        // TODO: Implement other KPI checks (failover time, fat link share, etc.)
        
        Ok(())
    }
}

#[derive(Clone)]
pub struct RunContext {
    pub run_id: String,
    pub outdir: PathBuf,
    pub links: usize,
    pub scenario: crate::scenarios::ScenarioKind,
    pub efficiency: f64,
    pub metrics: Metrics,
}
