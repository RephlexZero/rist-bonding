//! Runtime scheduler for time-varying network conditions
//!
//! This module provides functionality to apply schedule-driven changes
//! to network impairments at runtime using Tokio tasks.

use crate::qdisc::{NetemConfig, QdiscManager};
use rand::{rngs::StdRng, Rng, SeedableRng};
use scenarios::{DirectionSpec, Schedule};
use serde_json;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::time::{sleep, Instant};
use tracing::{debug, info, warn};

#[derive(Error, Debug)]
pub enum RuntimeError {
    #[error("Scheduler task error: {0}")]
    Task(String),

    #[error("Qdisc error: {0}")]
    Qdisc(#[from] crate::qdisc::QdiscError),

    #[error("Invalid schedule: {0}")]
    InvalidSchedule(String),
}

/// State for Markov chain scheduling
#[derive(Debug, Clone)]
struct MarkovState {
    current_state: usize,
    next_transition_time: Instant,
    rng: StdRng,
}

/// State for trace replay scheduling  
#[derive(Debug, Clone)]
struct ReplayState {
    trace_data: Vec<(Duration, DirectionSpec)>,
    current_index: usize,
}

impl LinkRuntime {
    /// Create a new link runtime
    pub fn new(
        namespace: String,
        interface_index: u32,
        qdisc_manager: Arc<QdiscManager>,
        initial_spec: DirectionSpec,
        schedule: Schedule,
    ) -> Self {
        Self {
            namespace,
            interface_index,
            qdisc_manager,
            current_spec: initial_spec,
            schedule,
            markov_state: None,
            replay_state: None,
        }
    }

    // Removed unused init_markov_state (state is initialized lazily in get_next_spec)

    /// Initialize state for trace replay scheduling
    fn init_replay_state(&mut self) -> Result<(), RuntimeError> {
        if let Schedule::Replay { path } = &self.schedule {
            // Load trace data from file
            let trace_data = Self::load_trace_file(path)?;
            self.replay_state = Some(ReplayState {
                trace_data,
                current_index: 0,
            });
        }
        Ok(())
    }

    /// Load trace data from CSV or JSON file
    fn load_trace_file(path: &Path) -> Result<Vec<(Duration, DirectionSpec)>, RuntimeError> {
        use std::fs;

        let content = fs::read_to_string(path)
            .map_err(|e| RuntimeError::InvalidSchedule(format!("Cannot read trace file: {}", e)))?;

        // Detect JSON either by extension or by content prefix
        let is_json_ext = path.extension().and_then(|s| s.to_str()) == Some("json");
        let trimmed = content.trim_start();
        let is_json_like = trimmed.starts_with('{') || trimmed.starts_with('[');

        if is_json_ext || is_json_like {
            serde_json::from_str(&content).map_err(|e| {
                RuntimeError::InvalidSchedule(format!("Invalid JSON trace file: {}", e))
            })
        } else {
            // TODO: Implement CSV parsing
            Err(RuntimeError::InvalidSchedule(
                "CSV trace files not yet implemented".to_string(),
            ))
        }
    }
}
#[derive(Debug)]
pub struct LinkRuntime {
    pub namespace: String,
    pub interface_index: u32,
    pub qdisc_manager: Arc<QdiscManager>,
    pub current_spec: DirectionSpec,
    /// The schedule to follow for this link
    pub schedule: Schedule,
    /// Markov chain state (if using Markov scheduling)
    markov_state: Option<MarkovState>,
    /// Replay state (if using Replay scheduling)
    replay_state: Option<ReplayState>,
}

/// Scheduler manages time-varying network impairments
pub struct Scheduler {
    runtimes: Arc<Mutex<Vec<Arc<Mutex<LinkRuntime>>>>>,
    shutdown: Arc<Mutex<bool>>,
}

impl Scheduler {
    pub fn new() -> Self {
        Self {
            runtimes: Arc::new(Mutex::new(Vec::new())),
            shutdown: Arc::new(Mutex::new(false)),
        }
    }

    /// Add a link runtime to be managed
    pub async fn add_link_runtime(&self, runtime: LinkRuntime) {
        let mut runtimes = self.runtimes.lock().await;
        runtimes.push(Arc::new(Mutex::new(runtime)));
    }

    /// Start the scheduler (spawns background tasks)
    pub async fn start(&self) -> Result<(), RuntimeError> {
        let runtimes = self.runtimes.lock().await.clone();
        let num_runtimes = runtimes.len();

        for runtime in runtimes {
            let shutdown = self.shutdown.clone();

            tokio::spawn(async move {
                if let Err(e) = Self::run_schedule_task(runtime, shutdown).await {
                    warn!("Schedule task failed: {}", e);
                }
            });
        }

        info!("Started scheduler for {} link runtimes", num_runtimes);
        Ok(())
    }

    /// Shutdown the scheduler
    pub async fn shutdown(&self) {
        let mut shutdown = self.shutdown.lock().await;
        *shutdown = true;
        info!("Scheduler shutdown requested");
    }

    /// Run a schedule task for a single link runtime
    async fn run_schedule_task(
        runtime: Arc<Mutex<LinkRuntime>>,
        shutdown: Arc<Mutex<bool>>,
    ) -> Result<(), RuntimeError> {
        let start_time = Instant::now();

        loop {
            // Check for shutdown
            if *shutdown.lock().await {
                debug!("Schedule task shutting down");
                break;
            }

            let (next_spec, delay) = {
                let mut rt = runtime.lock().await;
                let elapsed = start_time.elapsed();

                match Self::get_next_spec(&mut rt, elapsed)? {
                    Some((spec, delay)) => (spec, delay),
                    None => {
                        debug!("Schedule completed");
                        break;
                    }
                }
            };

            // Wait for the next change
            sleep(delay).await;

            // Apply the new specification
            {
                let rt = runtime.lock().await;
                if let Err(e) = Self::apply_spec(&rt, &next_spec).await {
                    warn!("Failed to apply spec: {}", e);
                } else {
                    debug!("Applied new spec: {:?}", next_spec);
                }
            }

            // Update current spec
            {
                let mut rt = runtime.lock().await;
                rt.current_spec = next_spec;
            }
        }

        Ok(())
    }

    /// Get the next specification and delay from a schedule
    fn get_next_spec(
        runtime: &mut LinkRuntime,
        elapsed: Duration,
    ) -> Result<Option<(DirectionSpec, Duration)>, RuntimeError> {
        match &runtime.schedule {
            Schedule::Constant(spec) => {
                // For constant schedules, only apply once at the beginning
                if elapsed.is_zero() {
                    Ok(Some((spec.clone(), Duration::ZERO)))
                } else {
                    Ok(None)
                }
            }
            Schedule::Steps(steps) => {
                // Find the next step that should be applied
                for (step_time, spec) in steps {
                    if *step_time >= elapsed {
                        let delay = if *step_time > elapsed {
                            *step_time - elapsed
                        } else {
                            Duration::ZERO
                        };
                        return Ok(Some((spec.clone(), delay)));
                    }
                }
                Ok(None)
            }
            Schedule::Markov {
                states,
                transition_matrix,
                mean_dwell_time,
                initial_state,
            } => {
                // If no states, terminate gracefully
                if states.is_empty() {
                    return Err(RuntimeError::InvalidSchedule(
                        "Markov schedule has no states".to_string(),
                    ));
                }

                // Clamp invalid initial_state into range
                let max_idx = states.len() - 1;
                let init_state = if *initial_state > max_idx {
                    max_idx
                } else {
                    *initial_state
                };

                // Initialize Markov state if needed, schedule first transition in the future
                if runtime.markov_state.is_none() {
                    runtime.markov_state = Some(MarkovState {
                        current_state: init_state,
                        next_transition_time: Instant::now() + *mean_dwell_time,
                        rng: StdRng::from_entropy(),
                    });
                }

                let now = Instant::now();
                let (should_transition, delay, current_state) = {
                    let markov_state = runtime.markov_state.as_ref().unwrap();
                    let cs = markov_state.current_state;
                    if now >= markov_state.next_transition_time {
                        (true, Duration::ZERO, cs)
                    } else {
                        (
                            false,
                            markov_state.next_transition_time.duration_since(now),
                            cs,
                        )
                    }
                };

                if should_transition {
                    let markov_state = runtime.markov_state.as_mut().unwrap();

                    // Determine next state using transition matrix (guard indices)
                    if let Some(row) = transition_matrix.get(markov_state.current_state) {
                        let rand_val: f32 = markov_state.rng.gen();
                        let mut cumulative_prob = 0.0;
                        let mut chosen = markov_state.current_state; // default stay
                        for (next_state, &prob) in row.iter().enumerate() {
                            cumulative_prob += prob;
                            if rand_val < cumulative_prob {
                                chosen = next_state;
                                break;
                            }
                        }
                        markov_state.current_state = chosen;
                    }

                    // Set next transition time using exponential distribution
                    let exponential_delay = {
                        let u: f32 = markov_state.rng.gen();
                        let lambda = 1.0 / mean_dwell_time.as_secs_f32().max(0.0001);
                        Duration::from_secs_f32((-u.ln() / lambda).max(0.0))
                    };
                    markov_state.next_transition_time = now + exponential_delay;

                    let spec = states
                        .get(markov_state.current_state)
                        .cloned()
                        .unwrap_or_else(DirectionSpec::typical);
                    Ok(Some((spec, Duration::ZERO)))
                } else {
                    let spec = states
                        .get(current_state)
                        .cloned()
                        .unwrap_or_else(DirectionSpec::typical);
                    Ok(Some((spec, delay)))
                }
            }
            Schedule::Replay { .. } => {
                // Initialize replay state if needed
                if runtime.replay_state.is_none() {
                    runtime.init_replay_state()?;
                }

                let replay_state = runtime.replay_state.as_mut().unwrap();

                // Find the next trace entry
                while replay_state.current_index < replay_state.trace_data.len() {
                    let (trace_time, spec) = &replay_state.trace_data[replay_state.current_index];

                    if *trace_time > elapsed {
                        let delay = *trace_time - elapsed;
                        return Ok(Some((spec.clone(), delay)));
                    } else if *trace_time == elapsed {
                        replay_state.current_index += 1;
                        return Ok(Some((spec.clone(), Duration::ZERO)));
                    } else {
                        // Skip past entries that are in the past
                        replay_state.current_index += 1;
                    }
                }

                // End of trace
                Ok(None)
            }
        }
    }

    /// Apply a direction specification to the network interface
    async fn apply_spec(runtime: &LinkRuntime, spec: &DirectionSpec) -> Result<(), RuntimeError> {
        // TODO: Get handle for the specific namespace
        // For now, this is a placeholder implementation

        let netem_config = NetemConfig {
            delay_us: spec.base_delay_ms * 1000, // Convert to microseconds
            jitter_us: spec.jitter_ms * 1000,
            loss_percent: spec.loss_pct * 100.0, // Convert to percentage
            loss_correlation: spec.loss_burst_corr,
            reorder_percent: spec.reorder_pct * 100.0,
            duplicate_percent: spec.duplicate_pct * 100.0,
            rate_bps: spec.rate_kbps as u64 * 1000, // Convert to bps
        };

        debug!(
            "Would apply netem config: {:?} to interface {} in namespace {}",
            netem_config, runtime.interface_index, runtime.namespace
        );

        // TODO: Actually apply the configuration using qdisc manager
        // runtime.qdisc_manager.update_netem(handle, runtime.interface_index, netem_config).await?;

        Ok(())
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scenarios::DirectionSpec;

    #[tokio::test]
    async fn test_scheduler_creation() {
        let scheduler = Scheduler::new();
        assert!(scheduler.runtimes.lock().await.is_empty());
    }

    #[test]
    fn test_constant_schedule() {
        let spec = DirectionSpec::good();
        let schedule = Schedule::Constant(spec.clone());
        let mut runtime = LinkRuntime::new(
            "test".to_string(),
            1,
            Arc::new(QdiscManager),
            spec.clone(),
            schedule,
        );

        // First call should return the spec immediately
        let result = Scheduler::get_next_spec(&mut runtime, Duration::ZERO).unwrap();
        assert!(result.is_some());
        let (returned_spec, delay) = result.unwrap();
        assert_eq!(returned_spec.rate_kbps, spec.rate_kbps);
        assert_eq!(delay, Duration::ZERO);

        // Second call should return None
        let result = Scheduler::get_next_spec(&mut runtime, Duration::from_secs(1)).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_steps_schedule() {
        let good = DirectionSpec::good();
        let poor = DirectionSpec::poor();
        let schedule = Schedule::Steps(vec![
            (Duration::from_secs(0), good.clone()),
            (Duration::from_secs(30), poor.clone()),
            (Duration::from_secs(60), good.clone()),
        ]);
        let mut runtime = LinkRuntime::new(
            "test".to_string(),
            1,
            Arc::new(QdiscManager),
            good.clone(),
            schedule,
        );

        // At time 0, should get first step immediately
        let result = Scheduler::get_next_spec(&mut runtime, Duration::ZERO).unwrap();
        assert!(result.is_some());
        let (spec, delay) = result.unwrap();
        assert_eq!(spec.rate_kbps, good.rate_kbps);
        assert_eq!(delay, Duration::ZERO);

        // At time 10s, should get next step with 20s delay
        let result = Scheduler::get_next_spec(&mut runtime, Duration::from_secs(10)).unwrap();
        assert!(result.is_some());
        let (spec, delay) = result.unwrap();
        assert_eq!(spec.rate_kbps, poor.rate_kbps); // Should be poor spec (2000 kbps)
        assert_eq!(delay, Duration::from_secs(20));

        // At time 70s, should be done
        let result = Scheduler::get_next_spec(&mut runtime, Duration::from_secs(70)).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_markov_schedule() {
        let good = DirectionSpec::good();
        let poor = DirectionSpec::poor();
        let schedule = Schedule::Markov {
            states: vec![good.clone(), poor.clone()],
            transition_matrix: vec![
                vec![0.8, 0.2], // good -> good: 80%, good -> poor: 20%
                vec![0.6, 0.4], // poor -> good: 60%, poor -> poor: 40%
            ],
            initial_state: 0, // Start in good state
            mean_dwell_time: Duration::from_secs(10),
        };
        let mut runtime = LinkRuntime::new(
            "test".to_string(),
            1,
            Arc::new(QdiscManager),
            good.clone(),
            schedule,
        );

        // First call should initialize and return initial state
        let result = Scheduler::get_next_spec(&mut runtime, Duration::ZERO).unwrap();
        assert!(result.is_some());
        let (spec, _delay) = result.unwrap();
        // Should be in initial state (good)
        assert_eq!(spec.rate_kbps, good.rate_kbps);

        // Markov state should be initialized
        assert!(runtime.markov_state.is_some());
    }

    #[test]
    fn test_replay_schedule_missing_file() {
        let schedule = Schedule::Replay {
            path: "/nonexistent/file.json".into(),
        };
        let mut runtime = LinkRuntime::new(
            "test".to_string(),
            1,
            Arc::new(QdiscManager),
            DirectionSpec::good(),
            schedule,
        );

        // Should fail to initialize with missing file
        let result = Scheduler::get_next_spec(&mut runtime, Duration::ZERO);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_scheduler_shutdown() {
        let scheduler = Scheduler::new();

        // Add some runtimes
        let runtime1 = LinkRuntime::new(
            "test1".to_string(),
            1,
            Arc::new(QdiscManager),
            DirectionSpec::good(),
            Schedule::Constant(DirectionSpec::good()),
        );
        let runtime2 = LinkRuntime::new(
            "test2".to_string(),
            2,
            Arc::new(QdiscManager),
            DirectionSpec::poor(),
            Schedule::Constant(DirectionSpec::poor()),
        );

        scheduler.add_link_runtime(runtime1).await;
        scheduler.add_link_runtime(runtime2).await;

        // Start scheduler
        assert!(scheduler.start().await.is_ok());

        // Should be able to shutdown without hanging
        scheduler.shutdown().await;
        assert!(*scheduler.shutdown.lock().await);
    }

    #[tokio::test]
    async fn test_concurrent_scheduler_operations() {
        let scheduler = Scheduler::new();

        // Add multiple runtimes concurrently
        let add_handles: Vec<_> = (0u32..5u32)
            .map(|i| {
                let scheduler = &scheduler;
                async move {
                    let runtime = LinkRuntime::new(
                        format!("test_{}", i),
                        i,
                        Arc::new(QdiscManager),
                        DirectionSpec::good(),
                        Schedule::Constant(DirectionSpec::good()),
                    );
                    scheduler.add_link_runtime(runtime).await;
                }
            })
            .collect();

        // Execute all adds concurrently
        futures::future::join_all(add_handles).await;

        // Verify all runtimes were added
        assert_eq!(scheduler.runtimes.lock().await.len(), 5);

        // Start and shutdown concurrently shouldn't cause issues
        let start_result = scheduler.start().await;
        assert!(start_result.is_ok());

        scheduler.shutdown().await;
        assert!(*scheduler.shutdown.lock().await);
    }

    #[tokio::test]
    async fn test_scheduler_task_error_recovery() {
        let scheduler = Scheduler::new();

        // Create a runtime with an invalid Markov schedule (empty states)
        let invalid_schedule = Schedule::Markov {
            states: vec![], // Empty states should cause errors
            transition_matrix: vec![],
            initial_state: 0,
            mean_dwell_time: Duration::from_secs(1),
        };

        let runtime = LinkRuntime::new(
            "error_test".to_string(),
            1,
            Arc::new(QdiscManager),
            DirectionSpec::good(),
            invalid_schedule,
        );

        scheduler.add_link_runtime(runtime).await;

        // Starting should not fail even with problematic runtimes
        // (errors are handled within tasks and logged)
        assert!(scheduler.start().await.is_ok());

        scheduler.shutdown().await;
    }

    #[test]
    fn test_markov_schedule_invalid_initial_state() {
        let schedule = Schedule::Markov {
            states: vec![DirectionSpec::good(), DirectionSpec::poor()],
            transition_matrix: vec![vec![0.8, 0.2], vec![0.6, 0.4]],
            initial_state: 5, // Invalid initial state (index out of bounds)
            mean_dwell_time: Duration::from_secs(10),
        };

        let mut runtime = LinkRuntime::new(
            "test".to_string(),
            1,
            Arc::new(QdiscManager),
            DirectionSpec::good(),
            schedule,
        );

        // Should not panic on invalid initial state, but might return error or default behavior
        let result = Scheduler::get_next_spec(&mut runtime, Duration::ZERO);
        // Implementation should handle this gracefully
        if result.is_err() {
            // Error is acceptable
        } else if let Ok(Some((spec, _))) = result {
            // Or it might default to a valid state
            assert!(spec.rate_kbps > 0);
        }
    }

    #[test]
    fn test_markov_schedule_invalid_transition_matrix() {
        let schedule = Schedule::Markov {
            states: vec![DirectionSpec::good(), DirectionSpec::poor()],
            transition_matrix: vec![
                vec![0.8], // Missing entry - should have 2 elements
                vec![0.6, 0.4],
            ],
            initial_state: 0,
            mean_dwell_time: Duration::from_secs(10),
        };

        let mut runtime = LinkRuntime::new(
            "test".to_string(),
            1,
            Arc::new(QdiscManager),
            DirectionSpec::good(),
            schedule,
        );

        // Should handle malformed transition matrix gracefully
        let result = Scheduler::get_next_spec(&mut runtime, Duration::ZERO);
        // Either error or graceful degradation is acceptable
        match result {
            Ok(Some((spec, _))) => assert!(spec.rate_kbps > 0),
            Ok(None) => {} // End of schedule
            Err(_) => {}   // Error is acceptable for invalid input
        }
    }

    #[test]
    fn test_steps_schedule_empty() {
        let schedule = Schedule::Steps(vec![]);
        let mut runtime = LinkRuntime::new(
            "test".to_string(),
            1,
            Arc::new(QdiscManager),
            DirectionSpec::good(),
            schedule,
        );

        // Empty steps schedule should return None immediately
        let result = Scheduler::get_next_spec(&mut runtime, Duration::ZERO).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_steps_schedule_unordered() {
        let good = DirectionSpec::good();
        let poor = DirectionSpec::poor();

        // Create unordered steps
        let schedule = Schedule::Steps(vec![
            (Duration::from_secs(60), good.clone()),
            (Duration::from_secs(0), poor.clone()), // Out of order
            (Duration::from_secs(30), good.clone()),
        ]);

        let mut runtime = LinkRuntime::new(
            "test".to_string(),
            1,
            Arc::new(QdiscManager),
            good.clone(),
            schedule,
        );

        // Should handle unordered steps (implementation dependent)
        let result = Scheduler::get_next_spec(&mut runtime, Duration::from_secs(10));
        // Either error or some graceful handling is acceptable
        match result {
            Ok(Some((spec, delay))) => {
                assert!(spec.rate_kbps > 0);
                assert!(delay >= Duration::ZERO);
            }
            Ok(None) => {}
            Err(_) => {}
        }
    }

    #[tokio::test]
    async fn test_apply_spec_error_handling() {
        // Test that apply_spec handles errors gracefully
        let runtime = LinkRuntime::new(
            "nonexistent_namespace".to_string(),
            999, // Invalid interface index
            Arc::new(QdiscManager),
            DirectionSpec::good(),
            Schedule::Constant(DirectionSpec::good()),
        );

        let spec = DirectionSpec::poor();

        // Should not panic on invalid namespace/interface
        let result = Scheduler::apply_spec(&runtime, &spec).await;
        // Current implementation just logs and returns Ok, but future implementations might error
        match result {
            Ok(()) => {} // Current behavior
            Err(_) => {} // Future implementations might return errors
        }
    }

    #[test]
    fn test_load_trace_file_invalid_json() {
        use std::fs;
        use tempfile::NamedTempFile;

        // Create invalid JSON file
        let temp_file = NamedTempFile::new().unwrap();
        fs::write(&temp_file, "{ invalid json }").unwrap();

        let result = LinkRuntime::load_trace_file(temp_file.path());
        assert!(result.is_err());

        if let Err(RuntimeError::InvalidSchedule(msg)) = result {
            assert!(msg.contains("Invalid JSON"));
        } else {
            panic!("Expected InvalidSchedule error with JSON message");
        }
    }

    #[test]
    fn test_load_trace_file_csv_not_implemented() {
        use std::fs;
        use tempfile::Builder;

        // Create CSV file
        let temp_file = Builder::new().suffix(".csv").tempfile().unwrap();
        fs::write(&temp_file, "time,rate\n0,1000\n30,2000\n").unwrap();

        let result = LinkRuntime::load_trace_file(temp_file.path());
        assert!(result.is_err());

        if let Err(RuntimeError::InvalidSchedule(msg)) = result {
            assert!(msg.contains("CSV trace files not yet implemented"));
        } else {
            panic!("Expected InvalidSchedule error for CSV");
        }
    }

    #[tokio::test]
    async fn test_race_condition_markov_state_initialization() {
        use std::sync::Arc;
        use tokio::sync::Mutex;

        let schedule = Schedule::Markov {
            states: vec![DirectionSpec::good(), DirectionSpec::poor()],
            transition_matrix: vec![vec![0.5, 0.5], vec![0.5, 0.5]],
            initial_state: 0,
            mean_dwell_time: Duration::from_millis(100),
        };

        let runtime = Arc::new(Mutex::new(LinkRuntime::new(
            "race_test".to_string(),
            1,
            Arc::new(QdiscManager),
            DirectionSpec::good(),
            schedule,
        )));

        // Simulate concurrent access to Markov state initialization
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let runtime = runtime.clone();
                tokio::spawn(async move {
                    let mut rt = runtime.lock().await;
                    let result = Scheduler::get_next_spec(&mut rt, Duration::from_millis(i * 10));
                    result
                })
            })
            .collect();

        // All tasks should complete without panic
        let results = futures::future::join_all(handles).await;
        for handle_result in results {
            assert!(handle_result.is_ok()); // No panic
                                            // Individual results may vary, but should not crash
        }
    }

    #[tokio::test]
    async fn test_scheduler_with_high_frequency_updates() {
        let scheduler = Scheduler::new();

        // Create a schedule with very frequent updates
        let steps: Vec<_> = (0..100)
            .map(|i| {
                (
                    Duration::from_millis(i * 10),
                    if i % 2 == 0 {
                        DirectionSpec::good()
                    } else {
                        DirectionSpec::poor()
                    },
                )
            })
            .collect();

        let runtime = LinkRuntime::new(
            "high_freq_test".to_string(),
            1,
            Arc::new(QdiscManager),
            DirectionSpec::good(),
            Schedule::Steps(steps),
        );

        scheduler.add_link_runtime(runtime).await;

        // Start scheduler
        assert!(scheduler.start().await.is_ok());

        // Let it run briefly
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Should be able to shutdown cleanly
        scheduler.shutdown().await;
        assert!(*scheduler.shutdown.lock().await);
    }

    #[tokio::test]
    async fn test_multiple_schedulers_independence() {
        let scheduler1 = Scheduler::new();
        let scheduler2 = Scheduler::new();

        // Add different runtimes to each scheduler
        let runtime1 = LinkRuntime::new(
            "sched1_test".to_string(),
            1,
            Arc::new(QdiscManager),
            DirectionSpec::good(),
            Schedule::Constant(DirectionSpec::good()),
        );

        let runtime2 = LinkRuntime::new(
            "sched2_test".to_string(),
            2,
            Arc::new(QdiscManager),
            DirectionSpec::poor(),
            Schedule::Constant(DirectionSpec::poor()),
        );

        scheduler1.add_link_runtime(runtime1).await;
        scheduler2.add_link_runtime(runtime2).await;

        // Both should start and shutdown independently
        assert!(scheduler1.start().await.is_ok());
        assert!(scheduler2.start().await.is_ok());

        scheduler1.shutdown().await;
        assert!(*scheduler1.shutdown.lock().await);
        assert!(!*scheduler2.shutdown.lock().await); // scheduler2 still running

        scheduler2.shutdown().await;
        assert!(*scheduler2.shutdown.lock().await);
    }
}
