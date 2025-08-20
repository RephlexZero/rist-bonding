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

        if path.extension().and_then(|s| s.to_str()) == Some("json") {
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
                // Initialize Markov state if needed
                if runtime.markov_state.is_none() {
                    runtime.markov_state = Some(MarkovState {
                        current_state: *initial_state,
                        next_transition_time: Instant::now(),
                        rng: StdRng::from_entropy(),
                    });
                }

                let now = Instant::now();
                let should_transition;
                let delay;
                let current_state;

                {
                    let markov_state = runtime.markov_state.as_ref().unwrap();
                    current_state = markov_state.current_state;

                    if now >= markov_state.next_transition_time {
                        should_transition = true;
                        delay = Duration::ZERO;
                    } else {
                        should_transition = false;
                        delay = markov_state.next_transition_time.duration_since(now);
                    }
                }

                if should_transition {
                    let markov_state = runtime.markov_state.as_mut().unwrap();

                    // Determine next state using transition matrix
                    let transition_probs = &transition_matrix[current_state];
                    let rand_val: f32 = markov_state.rng.gen();

                    let mut cumulative_prob = 0.0;
                    for (next_state, &prob) in transition_probs.iter().enumerate() {
                        cumulative_prob += prob;
                        if rand_val < cumulative_prob {
                            markov_state.current_state = next_state;
                            break;
                        }
                    }

                    // Set next transition time using exponential distribution
                    let exponential_delay = {
                        let u: f32 = markov_state.rng.gen();
                        let lambda = 1.0 / mean_dwell_time.as_secs_f32();
                        Duration::from_secs_f32(-u.ln() / lambda)
                    };
                    markov_state.next_transition_time = now + exponential_delay;

                    let spec = states[markov_state.current_state].clone();
                    Ok(Some((spec, Duration::ZERO)))
                } else {
                    let spec = states[current_state].clone();
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
}
