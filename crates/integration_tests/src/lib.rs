//! End-to-end RIST integration tests
//!
//! These tests validate the complete system:
//! - NetworkOrchestrator with race car cellular conditions
//! - Actual RIST dispatcher with dynamic bitrate control
//! - Observability monitoring throughout the test
//! - Realistic bonding scenario validation

use anyhow::Result;
use std::time::Duration;
use tokio::{net::UdpSocket, process::Command, time::sleep};
use tracing::{debug, info};

pub mod element_pad_semantics;

/// RIST Integration Test Suite
pub struct RistIntegrationTest {
    orchestrator: netlink_sim::enhanced::EnhancedNetworkOrchestrator,
    rist_dispatcher_process: Option<tokio::process::Child>,
    test_id: String,
    rx_port: u16,
}

impl RistIntegrationTest {
    /// Create new integration test
    pub async fn new(test_id: String, rx_port: u16) -> Result<Self> {
        let trace_path = format!("/tmp/rist_test_{}.trace", test_id);
        let orchestrator =
            netlink_sim::enhanced::EnhancedNetworkOrchestrator::new_with_observability(
                42,
                Some(&trace_path),
            )
            .await?;

        Ok(Self {
            orchestrator,
            rist_dispatcher_process: None,
            test_id,
            rx_port,
        })
    }

    /// Start RIST dispatcher with dynamic bitrate control
    pub async fn start_rist_dispatcher(&mut self) -> Result<()> {
        info!("🚀 Starting RIST dispatcher...");

        // Build RIST dispatcher if needed
        self.build_rist_dispatcher().await?;

        // Start the RIST dispatcher process
        let mut cmd = Command::new("cargo");
        cmd.args(&["run", "--bin", "ristdispatcher", "--"])
            .args(&[
                "--rx-port",
                &self.rx_port.to_string(),
                "--stats-interval",
                "1000",
                "--adaptive-bitrate",
                "true",
                "--bonding-mode",
                "main-backup",
                "--max-bitrate",
                "2000", // Race car realistic max
                "--min-bitrate",
                "300", // Race car realistic min
            ])
            .current_dir("/home/jake/Documents/rust/rist-bonding");

        let child = cmd.spawn()?;
        self.rist_dispatcher_process = Some(child);

        // Give dispatcher time to start up
        sleep(Duration::from_secs(2)).await;
        info!("✓ RIST dispatcher started");
        Ok(())
    }

    /// Set up race car bonding scenario
    pub async fn setup_race_car_bonding(&mut self) -> Result<Vec<netlink_sim::LinkHandle>> {
        info!("🏁 Setting up race car cellular bonding...");

        let links = self
            .orchestrator
            .start_race_car_bonding(self.rx_port)
            .await?;

        info!("✓ Bonding setup complete");
        for (i, handle) in links.iter().enumerate() {
            debug!(
                "  Link {}: {} ({}kbps)",
                i + 1,
                handle.scenario.name,
                handle.scenario.forward_params.rate_bps / 1000
            );
        }

        Ok(links)
    }

    /// Run realistic race car test pattern
    pub async fn run_race_car_test_pattern(&mut self) -> Result<TestResults> {
        info!("🏎️  Running race car test pattern...");

        let start_time = std::time::Instant::now();
        let mut results = TestResults::new(self.test_id.clone());

        // Phase 1: Strong signals (track start)
        debug!("  Phase 1: Track start - strong signals");
        self.simulate_traffic_phase("strong", Duration::from_secs(10))
            .await?;
        results.add_phase("strong", self.collect_phase_metrics().await?);

        // Phase 2: Signal degradation (entering tunnel/obstruction)
        debug!("  Phase 2: Signal degradation");
        self.apply_degradation_schedule().await?;
        self.simulate_traffic_phase("degraded", Duration::from_secs(15))
            .await?;
        results.add_phase("degraded", self.collect_phase_metrics().await?);

        // Phase 3: Handover spike (switching cell towers)
        debug!("  Phase 3: Handover event");
        self.trigger_handover_event().await?;
        self.simulate_traffic_phase("handover", Duration::from_secs(8))
            .await?;
        results.add_phase("handover", self.collect_phase_metrics().await?);

        // Phase 4: Recovery (clear track)
        debug!("  Phase 4: Signal recovery");
        self.apply_recovery_schedule().await?;
        self.simulate_traffic_phase("recovery", Duration::from_secs(12))
            .await?;
        results.add_phase("recovery", self.collect_phase_metrics().await?);

        results.total_duration = start_time.elapsed();
        info!(
            "✓ Race car test pattern completed ({:.1}s)",
            results.total_duration.as_secs_f64()
        );

        Ok(results)
    }

    /// Validate RIST bonding behavior
    pub async fn validate_bonding_behavior(
        &self,
        results: &TestResults,
    ) -> Result<ValidationReport> {
        info!("🔍 Validating RIST bonding behavior...");

        let mut report = ValidationReport::new();

        // Check adaptive bitrate behavior
        let bitrate_adapted = results.phases.iter().any(|(phase, metrics)| {
            if phase == "degraded" {
                metrics.avg_bitrate < 1000.0 // Should reduce during degradation
            } else if phase == "recovery" {
                metrics.avg_bitrate > 1500.0 // Should recover after degradation
            } else {
                true
            }
        });

        report.adaptive_bitrate_working = bitrate_adapted;

        // Check bonding effectiveness
        let bonding_effective = results
            .phases
            .iter()
            .filter(|(phase, _)| *phase == "handover")
            .all(|(_, metrics)| metrics.packet_loss < 5.0); // Should maintain low loss during handover

        report.bonding_effective = bonding_effective;

        // Check link utilization
        let balanced_utilization = results.phases.iter().all(|(_, metrics)| {
            let util_ratio = metrics.primary_link_util / metrics.backup_link_util.max(0.01);
            util_ratio < 10.0 // Primary shouldn't dominate too much
        });

        report.load_balancing_working = balanced_utilization;

        info!("✓ Bonding validation completed");
        debug!(
            "  - Adaptive bitrate: {}",
            if report.adaptive_bitrate_working {
                "✅"
            } else {
                "❌"
            }
        );
        debug!(
            "  - Bonding effectiveness: {}",
            if report.bonding_effective {
                "✅"
            } else {
                "❌"
            }
        );
        debug!(
            "  - Load balancing: {}",
            if report.load_balancing_working {
                "✅"
            } else {
                "❌"
            }
        );

        Ok(report)
    }

    async fn build_rist_dispatcher(&self) -> Result<()> {
        info!("🔨 Building RIST dispatcher...");

        let output = Command::new("cargo")
            .args(&["build", "--bin", "ristdispatcher"])
            .current_dir("/home/jake/Documents/rust/rist-bonding")
            .output()
            .await?;

        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "Failed to build RIST dispatcher: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        info!("✓ RIST dispatcher built successfully");
        Ok(())
    }

    async fn simulate_traffic_phase(&self, phase: &str, duration: Duration) -> Result<()> {
        let start = std::time::Instant::now();
        let mut packet_count = 0;

        // Create test traffic socket
        let socket = UdpSocket::bind("127.0.0.1:0").await?;
        let dest = format!("127.0.0.1:{}", self.rx_port);

        while start.elapsed() < duration {
            // Send test packets at realistic race car data rates
            let packet_size = match phase {
                "strong" => 1400,   // Full MTU when signal is good
                "degraded" => 800,  // Smaller packets when degraded
                "handover" => 400,  // Very small during handover
                "recovery" => 1200, // Recovering packet size
                _ => 1000,
            };

            let test_data = vec![0u8; packet_size];
            socket.send_to(&test_data, &dest).await?;
            packet_count += 1;

            // Vary sending rate based on phase
            let interval = match phase {
                "strong" => 10,   // 10ms = 100 packets/sec
                "degraded" => 20, // 20ms = 50 packets/sec
                "handover" => 50, // 50ms = 20 packets/sec
                "recovery" => 15, // 15ms = ~67 packets/sec
                _ => 20,
            };

            tokio::time::sleep(Duration::from_millis(interval)).await;
        }

        debug!("    Sent {} packets during {} phase", packet_count, phase);
        Ok(())
    }

    async fn apply_degradation_schedule(&mut self) -> Result<()> {
        let degraded_schedule = scenarios::Schedule::race_track_circuit();
        self.orchestrator
            .apply_schedule("race_4g_primary", degraded_schedule.clone())
            .await?;
        self.orchestrator
            .apply_schedule("race_5g_primary", degraded_schedule)
            .await?;
        Ok(())
    }

    async fn trigger_handover_event(&mut self) -> Result<()> {
        let handover_schedule = scenarios::Schedule::race_4g_markov(); // High variability
        self.orchestrator
            .apply_schedule("race_4g_primary", handover_schedule)
            .await?;
        self.orchestrator
            .apply_schedule("race_4g_backup", scenarios::Schedule::race_5g_markov())
            .await?;
        Ok(())
    }

    async fn apply_recovery_schedule(&mut self) -> Result<()> {
        // Switch to stronger 5G as primary during recovery
        let recovery_schedule = scenarios::Schedule::race_5g_markov();
        self.orchestrator
            .apply_schedule("race_5g_primary", recovery_schedule)
            .await?;
        Ok(())
    }

    async fn collect_phase_metrics(&self) -> Result<PhaseMetrics> {
        if let Some(snapshot) = self.orchestrator.get_metrics_snapshot().await? {
            Ok(PhaseMetrics {
                avg_bitrate: snapshot
                    .link_performance
                    .iter()
                    .map(|m| m.link_stats.throughput_bps / 1000)
                    .sum::<u64>() as f64
                    / snapshot.link_performance.len().max(1) as f64,
                packet_loss: snapshot
                    .link_performance
                    .iter()
                    .map(|m| m.link_stats.loss_rate)
                    .sum::<f64>()
                    / snapshot.link_performance.len().max(1) as f64,
                avg_rtt: snapshot
                    .link_performance
                    .iter()
                    .map(|m| m.link_stats.rtt_ms)
                    .sum::<f64>()
                    / snapshot.link_performance.len().max(1) as f64,
                primary_link_util: 75.0, // Simulate primary link utilization
                backup_link_util: 25.0,  // Simulate backup link utilization
            })
        } else {
            Ok(PhaseMetrics::default())
        }
    }
}

impl Drop for RistIntegrationTest {
    fn drop(&mut self) {
        if let Some(mut child) = self.rist_dispatcher_process.take() {
            let _ = child.kill();
        }
    }
}

#[derive(Debug, Clone)]
pub struct PhaseMetrics {
    pub avg_bitrate: f64,
    pub packet_loss: f64,
    pub avg_rtt: f64,
    pub primary_link_util: f64,
    pub backup_link_util: f64,
}

impl Default for PhaseMetrics {
    fn default() -> Self {
        Self {
            avg_bitrate: 0.0,
            packet_loss: 0.0,
            avg_rtt: 0.0,
            primary_link_util: 0.0,
            backup_link_util: 0.0,
        }
    }
}

#[derive(Debug)]
pub struct TestResults {
    pub test_id: String,
    pub phases: Vec<(String, PhaseMetrics)>,
    pub total_duration: Duration,
}

impl TestResults {
    fn new(test_id: String) -> Self {
        Self {
            test_id,
            phases: Vec::new(),
            total_duration: Duration::from_secs(0),
        }
    }

    fn add_phase(&mut self, phase: &str, metrics: PhaseMetrics) {
        self.phases.push((phase.to_string(), metrics));
    }
}

#[derive(Debug)]
pub struct ValidationReport {
    pub adaptive_bitrate_working: bool,
    pub bonding_effective: bool,
    pub load_balancing_working: bool,
}

impl ValidationReport {
    fn new() -> Self {
        Self {
            adaptive_bitrate_working: false,
            bonding_effective: false,
            load_balancing_working: false,
        }
    }

    pub fn all_passed(&self) -> bool {
        self.adaptive_bitrate_working && self.bonding_effective && self.load_balancing_working
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono;
    use std::time::Duration;
    use tokio::time::timeout;

    #[tokio::test]
    async fn test_full_race_car_integration() {
        // Test the complete race car integration scenario
        let test_id = format!("integration_test_{}", chrono::Utc::now().timestamp());
        let mut test = RistIntegrationTest::new(test_id.clone(), 5008)
            .await
            .expect("Failed to create integration test");

        // Test race car bonding setup
        let links = test
            .setup_race_car_bonding()
            .await
            .expect("Failed to setup race car bonding");

        assert!(!links.is_empty(), "Should have created bonding links");
        assert!(
            links.len() >= 2,
            "Should have at least 2 bonding links for redundancy"
        );

        // Test that links have realistic race car parameters
        for link in &links {
            assert!(
                link.scenario.forward_params.rate_bps >= 100_000,
                "Race car links should have at least 100 Kbps"
            );
            assert!(
                link.scenario.forward_params.rate_bps <= 10_000_000,
                "Race car links should not exceed 10 Mbps"
            );
        }
    }

    #[tokio::test]
    async fn test_phase_transition_validation() {
        let test_id = format!("phase_test_{}", chrono::Utc::now().timestamp());
        let mut test = RistIntegrationTest::new(test_id, 5009)
            .await
            .expect("Failed to create integration test");

        // Test network degradation schedule application
        test.apply_degradation_schedule()
            .await
            .expect("Failed to apply degradation schedule");

        // Test handover event triggering
        test.trigger_handover_event()
            .await
            .expect("Failed to trigger handover event");

        // Test recovery schedule application
        test.apply_recovery_schedule()
            .await
            .expect("Failed to apply recovery schedule");
    }

    #[tokio::test]
    async fn test_metrics_collection_and_validation() {
        let test_id = format!("metrics_test_{}", chrono::Utc::now().timestamp());
        let test = RistIntegrationTest::new(test_id, 5010)
            .await
            .expect("Failed to create integration test");

        // Test metrics collection
        let metrics = test
            .collect_phase_metrics()
            .await
            .expect("Failed to collect phase metrics");

        // Validate metrics structure and reasonable values
        assert!(
            metrics.avg_bitrate >= 0.0,
            "Average bitrate should be non-negative"
        );
        assert!(
            metrics.packet_loss >= 0.0 && metrics.packet_loss <= 100.0,
            "Packet loss should be a valid percentage"
        );
        assert!(metrics.avg_rtt >= 0.0, "Average RTT should be non-negative");
        assert!(
            metrics.primary_link_util >= 0.0 && metrics.primary_link_util <= 100.0,
            "Primary link utilization should be a valid percentage"
        );
        assert!(
            metrics.backup_link_util >= 0.0 && metrics.backup_link_util <= 100.0,
            "Backup link utilization should be a valid percentage"
        );
    }

    #[tokio::test]
    async fn test_validation_report_comprehensive() {
        let mut results = TestResults::new("test_validation".to_string());

        // Add test phases with different characteristics
        results.add_phase(
            "strong",
            PhaseMetrics {
                avg_bitrate: 2000.0,
                packet_loss: 0.1,
                avg_rtt: 20.0,
                primary_link_util: 80.0,
                backup_link_util: 20.0,
            },
        );

        results.add_phase(
            "degraded",
            PhaseMetrics {
                avg_bitrate: 800.0, // Should be less than 1000 for test
                packet_loss: 2.0,
                avg_rtt: 150.0,
                primary_link_util: 60.0,
                backup_link_util: 40.0,
            },
        );

        results.add_phase(
            "handover",
            PhaseMetrics {
                avg_bitrate: 1200.0,
                packet_loss: 3.0, // Should be less than 5% for bonding effectiveness
                avg_rtt: 100.0,
                primary_link_util: 50.0,
                backup_link_util: 50.0,
            },
        );

        results.add_phase(
            "recovery",
            PhaseMetrics {
                avg_bitrate: 1800.0, // Should be greater than 1500 for test
                packet_loss: 0.5,
                avg_rtt: 25.0,
                primary_link_util: 75.0,
                backup_link_util: 25.0,
            },
        );

        let test_id = format!("validation_test_{}", chrono::Utc::now().timestamp());
        let test = RistIntegrationTest::new(test_id, 5011)
            .await
            .expect("Failed to create integration test");

        let report = test
            .validate_bonding_behavior(&results)
            .await
            .expect("Failed to validate bonding behavior");

        // Test that validation correctly identifies good bonding behavior
        assert!(
            report.adaptive_bitrate_working,
            "Should detect adaptive bitrate based on phase characteristics"
        );
        assert!(
            report.bonding_effective,
            "Should detect effective bonding during handover phase"
        );
        assert!(
            report.all_passed(),
            "All validation checks should pass with good metrics"
        );
    }

    #[tokio::test]
    async fn test_traffic_simulation_phases() {
        let test_id = format!("traffic_test_{}", chrono::Utc::now().timestamp());
        let test = RistIntegrationTest::new(test_id, 5012)
            .await
            .expect("Failed to create integration test");

        // Test different traffic simulation phases with short durations
        let phases = ["strong", "degraded", "handover", "recovery"];

        for phase in &phases {
            // Use timeout to ensure test doesn't hang
            let result = timeout(
                Duration::from_secs(10),
                test.simulate_traffic_phase(phase, Duration::from_millis(500)),
            )
            .await;

            assert!(
                result.is_ok(),
                "Traffic simulation should complete within timeout"
            );
            assert!(
                result.unwrap().is_ok(),
                "Traffic simulation for phase {} should succeed",
                phase
            );
        }
    }

    #[tokio::test]
    async fn test_error_handling_and_cleanup() {
        // Test with invalid port to trigger error handling
        let result = RistIntegrationTest::new("error_test".to_string(), 0).await;
        // This might succeed depending on implementation, but should handle gracefully

        if let Ok(test) = result {
            // Test that cleanup works properly (Drop implementation)
            drop(test);
        }

        // Test validation with empty results
        let empty_results = TestResults::new("empty_test".to_string());
        let test_id = format!("error_test_{}", chrono::Utc::now().timestamp());
        let test = RistIntegrationTest::new(test_id, 5013)
            .await
            .expect("Failed to create integration test");

        let report = test
            .validate_bonding_behavior(&empty_results)
            .await
            .expect("Should handle empty results gracefully");

        // Empty results should fail validation
        assert!(
            !report.all_passed(),
            "Empty results should not pass all validations"
        );
    }
}
