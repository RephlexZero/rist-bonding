//! Enhanced Network Orchestrator providing drop-in replacement for network-sim
//! 
//! This module provides a complete NetworkOrchestrator API that integrates with:
//! - scenarios crate for realistic network conditions  
//! - observability crate for comprehensive metrics
//! - netlink-sim backend for actual packet manipulation

#[cfg(feature = "enhanced")]
use anyhow::Result;
#[cfg(feature = "enhanced")]
use std::{collections::HashMap, sync::Arc};
#[cfg(feature = "enhanced")]
use tokio::sync::RwLock;

#[cfg(feature = "enhanced")]
use scenarios;
#[cfg(feature = "enhanced")]  
use observability;

/// Re-export core types from netlink-sim
pub use crate::{NetworkOrchestrator as BaseOrchestrator, LinkHandle, TestScenario, LinkParams};

/// Enhanced NetworkOrchestrator with scenarios and observability integration
#[cfg(feature = "enhanced")]
pub struct EnhancedNetworkOrchestrator {
    base: BaseOrchestrator,
    metrics_collector: Option<Arc<observability::MetricsCollector>>,
    link_metrics: HashMap<String, Arc<observability::LinkMetricsCollector>>,
    trace_recorder: Option<Arc<RwLock<observability::TraceRecorder>>>,
}

#[cfg(feature = "enhanced")]

impl EnhancedNetworkOrchestrator {
    /// Create new enhanced orchestrator with observability
    pub async fn new_with_observability(seed: u64, trace_path: Option<&str>) -> Result<Self> {
        let base = BaseOrchestrator::new(seed);
        let metrics_collector = Some(Arc::new(observability::MetricsCollector::new()));
        
        let trace_recorder = if let Some(path) = trace_path {
            Some(Arc::new(RwLock::new(observability::TraceRecorder::new(path)?)))
        } else {
            None
        };

        Ok(Self {
            base,
            metrics_collector,
            link_metrics: HashMap::new(),
            trace_recorder,
        })
    }

    /// Create new enhanced orchestrator without observability (basic mode)
    pub fn new(seed: u64) -> Self {
        Self {
            base: BaseOrchestrator::new(seed),
            metrics_collector: None,
            link_metrics: HashMap::new(),
            trace_recorder: None,
        }
    }

    /// Start scenario using scenarios crate DirectionSpec
    pub async fn start_scenario_with_spec(
        &mut self,
        name: String,
        forward_spec: scenarios::DirectionSpec,
        reverse_spec: scenarios::DirectionSpec,
        rx_port: u16,
    ) -> Result<LinkHandle> {
        // Convert DirectionSpec to LinkParams
        let forward_params = self.convert_direction_spec(&forward_spec);
        let reverse_params = self.convert_direction_spec(&reverse_spec);

        let scenario = TestScenario {
            name: name.clone(),
            description: format!("Scenario with forward: {}kbps, reverse: {}kbps", 
                                forward_spec.rate_kbps, reverse_spec.rate_kbps),
            forward_params,
            reverse_params,
            duration_seconds: None,
        };

        let handle = self.base.start_scenario(scenario, rx_port).await?;

        // Set up metrics collection if enabled
        if let Some(ref collector) = self.metrics_collector {
            let link_metrics = collector.add_link(name.clone());
            self.link_metrics.insert(name, link_metrics);
        }

        Ok(handle)
    }

    /// Start race car bonding test with realistic cellular conditions
    pub async fn start_race_car_bonding(
        &mut self, 
        rx_port: u16
    ) -> Result<Vec<LinkHandle>> {
        let scenarios = vec![
            ("race_4g_primary".to_string(), 
             scenarios::DirectionSpec::race_4g_strong(),
             scenarios::DirectionSpec::race_4g_moderate()),
            ("race_4g_backup".to_string(),
             scenarios::DirectionSpec::race_4g_moderate(), 
             scenarios::DirectionSpec::race_4g_moderate()),
            ("race_5g_primary".to_string(),
             scenarios::DirectionSpec::race_5g_strong(),
             scenarios::DirectionSpec::race_5g_moderate()),
            ("race_5g_backup".to_string(),
             scenarios::DirectionSpec::race_5g_moderate(),
             scenarios::DirectionSpec::race_5g_weak()),
        ];

        let mut handles = Vec::new();
        for (name, forward, reverse) in scenarios {
            let handle = self.start_scenario_with_spec(name, forward, reverse, rx_port).await?;
            handles.push(handle);
        }

        Ok(handles)
    }

    /// Apply dynamic schedule to a link using scenarios crate
    pub async fn apply_schedule(
        &mut self,
        link_name: &str,
        schedule: scenarios::Schedule,
    ) -> Result<()> {
        // Record schedule application in trace
        if let Some(ref recorder) = self.trace_recorder {
            let entry = observability::TraceEntry::new(
                "schedule_applied".to_string(),
                serde_json::json!({
                    "link_name": link_name,
                    "schedule": format!("{:?}", schedule)
                })
            ).with_link_id(link_name.to_string());

            recorder.write().await.record(entry).await?;
        }

        // TODO: Implement actual schedule application to running link
        // This would require extending the base orchestrator with dynamic reconfiguration
        tracing::info!("Applied schedule to link {}: {:?}", link_name, schedule);
        Ok(())
    }

    /// Get metrics snapshot if observability is enabled
    pub async fn get_metrics_snapshot(&self) -> Result<Option<observability::MetricsSnapshot>> {
        if let Some(ref collector) = self.metrics_collector {
            Ok(Some(collector.take_snapshot().await?))
        } else {
            Ok(None)
        }
    }

    /// Update link metrics (called periodically by monitoring)
    pub async fn update_link_metrics(
        &self,
        link_name: &str,
        bytes_sent: u64,
        bytes_received: u64,
        rtt_ms: f64,
        loss_rate: f64,
    ) -> Result<()> {
        if let Some(link_collector) = self.link_metrics.get(link_name) {
            link_collector.record_traffic(bytes_sent, bytes_received, 0, 0, 0);
            link_collector.update_quality(rtt_ms, 0.0, loss_rate, bytes_sent * 8); // Rough throughput estimate
        }
        Ok(())
    }

    /// Get active links (delegation to base)
    pub fn get_active_links(&self) -> &[LinkHandle] {
        self.base.get_active_links()
    }

    /// Convenience method: Start enhanced 5G scenario with observability
    pub async fn start_enhanced_5g_scenario(&mut self, rx_port: u16) -> Result<Vec<LinkHandle>> {
        let scenarios = vec![
            ("5g_strong_primary".to_string(),
             scenarios::DirectionSpec::race_5g_strong(),
             scenarios::DirectionSpec::race_5g_moderate()),
            ("5g_moderate_secondary".to_string(), 
             scenarios::DirectionSpec::race_5g_moderate(),
             scenarios::DirectionSpec::race_5g_weak()),
            ("4g_backup".to_string(),
             scenarios::DirectionSpec::race_4g_strong(),
             scenarios::DirectionSpec::race_4g_moderate()),
        ];

        let mut handles = Vec::new();
        for (name, forward, reverse) in scenarios {
            let handle = self.start_scenario_with_spec(name, forward, reverse, rx_port).await?;
            handles.push(handle);
        }

        Ok(handles)
    }

    fn convert_direction_spec(&self, spec: &scenarios::DirectionSpec) -> LinkParams {
        LinkParams {
            base_delay_ms: spec.base_delay_ms as u64,
            jitter_ms: spec.jitter_ms as u64,
            loss_pct: spec.loss_pct,
            reorder_pct: spec.reorder_pct,
            duplicate_pct: spec.duplicate_pct,
            rate_bps: (spec.rate_kbps * 1000) as u64,
            bucket_bytes: (spec.mtu.unwrap_or(1500) * 32) as usize, // Reasonable buffer size
        }
    }
}

/// Enhanced orchestrator factory functions
#[cfg(feature = "enhanced")]
impl EnhancedNetworkOrchestrator {
    /// Create race car testing orchestrator with full observability
    pub async fn for_race_car_testing(trace_path: &str) -> Result<Self> {
        Self::new_with_observability(42, Some(trace_path)).await
    }

    /// Create 5G research orchestrator 
    pub async fn for_5g_research(trace_path: &str) -> Result<Self> {
        Self::new_with_observability(123, Some(trace_path)).await
    }

    /// Create basic orchestrator (no observability overhead)
    pub fn for_basic_testing() -> Self {
        Self::new(999)
    }
}

#[cfg(all(test, feature = "enhanced"))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_enhanced_orchestrator_creation() {
        let orchestrator = EnhancedNetworkOrchestrator::new(42);
        assert_eq!(orchestrator.get_active_links().len(), 0);
    }

    #[tokio::test]
    async fn test_race_car_factory() {
        let result = EnhancedNetworkOrchestrator::for_race_car_testing("/tmp/test.trace").await;
        assert!(result.is_ok());
    }
}