//! Gilbert-Elliott model for burst loss patterns

use crate::errors::Result;
use crate::types::{GEParams, GeState};
use rand::Rng;
use std::time::Duration;
use tokio::time;
use tracing::{debug, warn};

/// Gilbert-Elliott controller state
#[derive(Debug)]
pub struct GEController {
    params: GEParams,
    state: GeState,
    last_tick: std::time::Instant,
    rng: Option<rand::rngs::StdRng>,
}

impl GEController {
    pub fn new(params: GEParams) -> Self {
        use rand::SeedableRng;

        let rng = if let Some(seed) = params.seed {
            Some(rand::rngs::StdRng::seed_from_u64(seed))
        } else {
            None
        };

        Self {
            params,
            state: GeState::Good,
            last_tick: std::time::Instant::now(),
            rng,
        }
    }

    /// Update parameters (can be called from async context)
    pub fn update_params(&mut self, params: GEParams) {
        debug!(
            "Updating GE parameters: p_good={}, p_bad={}, p={}, r={}",
            params.p_good, params.p_bad, params.p, params.r
        );
        self.params = params;
    }

    /// Get current loss probability based on state
    pub fn loss_probability(&self) -> f64 {
        match self.state {
            GeState::Good => self.params.p_good,
            GeState::Bad => self.params.p_bad,
        }
    }

    /// Advance the state machine based on time
    pub fn tick(&mut self) -> GeState {
        let now = std::time::Instant::now();
        let _elapsed = now.duration_since(self.last_tick);
        self.last_tick = now;

        // Use seeded RNG if available, otherwise use thread_rng
        let rand_val: f64 = if let Some(ref mut rng) = self.rng {
            rng.gen()
        } else {
            let mut thread_rng = rand::thread_rng();
            thread_rng.gen()
        };

        match self.state {
            GeState::Good => {
                if rand_val < self.params.p {
                    self.state = GeState::Bad;
                    debug!("GE state: Good -> Bad");
                }
            }
            GeState::Bad => {
                if rand_val < self.params.r {
                    self.state = GeState::Good;
                    debug!("GE state: Bad -> Good");
                }
            }
        }

        self.state
    }

    /// Check if a packet should be dropped based on current state
    pub fn should_drop_packet(&mut self) -> bool {
        // Use seeded RNG if available, otherwise use thread_rng
        let rand_val: f64 = if let Some(ref mut rng) = self.rng {
            rng.gen()
        } else {
            let mut thread_rng = rand::thread_rng();
            thread_rng.gen()
        };
        rand_val < self.loss_probability()
    }

    pub fn current_state(&self) -> GeState {
        self.state
    }

    pub fn params(&self) -> &GEParams {
        &self.params
    }
}

/// Manager for GE controllers across multiple links
#[derive(Debug)]
pub struct GEManager {
    controllers: std::collections::HashMap<String, tokio::task::JoinHandle<()>>,
    shutdown_txs: std::collections::HashMap<String, tokio::sync::oneshot::Sender<()>>,
}

impl Default for GEManager {
    fn default() -> Self {
        Self::new()
    }
}

impl GEManager {
    pub fn new() -> Self {
        Self {
            controllers: std::collections::HashMap::new(),
            shutdown_txs: std::collections::HashMap::new(),
        }
    }

    pub async fn start_controller(&mut self, link_name: String, params: GEParams) -> Result<()> {
        // Stop existing controller if any
        self.stop_controller(&link_name).await?;

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let handle =
            spawn_ge_controller(params, None, 100, shutdown_rx, |_state, _loss_prob| Ok(()));

        self.controllers.insert(link_name.clone(), handle);
        self.shutdown_txs.insert(link_name, shutdown_tx);

        Ok(())
    }

    pub async fn stop_controller(&mut self, link_name: &str) -> Result<()> {
        if let Some(shutdown_tx) = self.shutdown_txs.remove(link_name) {
            let _ = shutdown_tx.send(());
        }

        if let Some(handle) = self.controllers.remove(link_name) {
            handle.abort();
            let _ = handle.await; // Ignore cancellation errors
        }

        Ok(())
    }

    pub async fn stop_all(&mut self) -> Result<()> {
        let link_names: Vec<_> = self.controllers.keys().cloned().collect();
        for link_name in link_names {
            if let Err(e) = self.stop_controller(&link_name).await {
                warn!("Error stopping GE controller for {}: {}", link_name, e);
            }
        }
        Ok(())
    }

    pub fn is_running(&self, link_name: &str) -> bool {
        self.controllers.contains_key(link_name)
    }
}

impl Drop for GEManager {
    fn drop(&mut self) {
        // Send shutdown signals to all controllers
        for (_, shutdown_tx) in self.shutdown_txs.drain() {
            let _ = shutdown_tx.send(());
        }
    }
}

/// Spawn a GE controller task with callback support
pub fn spawn_ge_controller<F>(
    params: GEParams,
    _seed: Option<u64>, // NOTE: seed is now embedded in params.seed, this parameter is kept for backwards compatibility but not used
    tick_ms: u64,
    shutdown_rx: tokio::sync::oneshot::Receiver<()>,
    callback: F,
) -> tokio::task::JoinHandle<()>
where
    F: Fn(GeState, f64) -> Result<()> + Send + 'static,
{
    tokio::spawn(async move {
        let mut controller = GEController::new(params);
        let mut interval = time::interval(Duration::from_millis(tick_ms));
        let mut shutdown_rx = shutdown_rx;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let state = controller.tick();
                    let loss_prob = controller.loss_probability();

                    if let Err(e) = callback(state, loss_prob) {
                        warn!("GE callback error: {}", e);
                    }
                }
                _ = &mut shutdown_rx => {
                    debug!("GE controller shutting down");
                    break;
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::GEParams;

    #[test]
    fn test_ge_controller_creation() {
        let params = GEParams::default();
        let controller = GEController::new(params);
        assert_eq!(controller.current_state(), GeState::Good);
    }

    #[test]
    fn test_loss_probability() {
        let params = GEParams {
            p_good: 0.01,
            p_bad: 0.1,
            p: 0.05,
            r: 0.1,
            seed: None,
        };
        let controller = GEController::new(params);
        assert_eq!(controller.loss_probability(), 0.01);
    }

    #[test]
    fn test_ge_state_transitions_deterministic() {
        // Test state transitions with deterministic seed
        let params = GEParams {
            p_good: 0.05,   // 5% Good->Bad transition probability
            p_bad: 0.2,     // 20% Bad->Good transition probability
            p: 0.3,         // 30% chance Good->Bad
            r: 0.4,         // 40% chance Bad->Good
            seed: Some(42), // Fixed seed for reproducibility
        };

        let mut controller = GEController::new(params);
        assert_eq!(controller.current_state(), GeState::Good);

        // Track state transitions over many ticks
        let mut states = vec![controller.current_state()];
        for _ in 0..50 {
            let new_state = controller.tick();
            states.push(new_state);
        }

        // Should have both Good and Bad states
        let good_count = states.iter().filter(|&&s| s == GeState::Good).count();
        let bad_count = states.iter().filter(|&&s| s == GeState::Bad).count();

        assert!(
            good_count > 0,
            "Should have Good states: good={}, bad={}",
            good_count,
            bad_count
        );
        assert!(
            bad_count > 0,
            "Should have Bad states: good={}, bad={}",
            good_count,
            bad_count
        );

        // Test reproducibility with same seed
        let mut controller2 = GEController::new(GEParams {
            p_good: 0.05,
            p_bad: 0.2,
            p: 0.3,
            r: 0.4,
            seed: Some(42), // Same seed
        });

        let mut states2 = vec![controller2.current_state()];
        for _ in 0..50 {
            let new_state = controller2.tick();
            states2.push(new_state);
        }

        // Should get identical sequence with same seed
        assert_eq!(
            states, states2,
            "Deterministic seed should produce identical state sequences"
        );
    }

    #[test]
    fn test_ge_transition_probabilities() {
        // Test with high transition probabilities to ensure transitions occur
        let params = GEParams {
            p_good: 0.01, // Loss probability in Good state
            p_bad: 0.5,   // Loss probability in Bad state
            p: 0.8,       // High chance Good->Bad (80%)
            r: 0.9,       // High chance Bad->Good (90%)
            seed: Some(12345),
        };

        let mut controller = GEController::new(params);

        // Start in Good state
        assert_eq!(controller.current_state(), GeState::Good);
        assert_eq!(controller.loss_probability(), 0.01);

        // Force transitions by calling tick many times
        let mut transition_count = 0;
        let mut prev_state = controller.current_state();

        for _ in 0..100 {
            let new_state = controller.tick();
            if new_state != prev_state {
                transition_count += 1;
            }
            prev_state = new_state;
        }

        // With high transition probabilities, should have many transitions
        assert!(
            transition_count > 10,
            "Should have multiple transitions with high probabilities: {}",
            transition_count
        );
    }

    #[test]
    fn test_ge_loss_probability_in_states() {
        let params = GEParams {
            p_good: 0.001, // 0.1% loss in Good
            p_bad: 0.15,   // 15% loss in Bad
            p: 0.5,        // Equal transition probabilities
            r: 0.5,
            seed: Some(999),
        };

        let mut controller = GEController::new(params);

        // Initially in Good state
        assert_eq!(controller.current_state(), GeState::Good);
        assert_eq!(controller.loss_probability(), 0.001);

        // Force state changes and verify loss probabilities
        let mut seen_good = false;
        let mut seen_bad = false;

        for _ in 0..200 {
            controller.tick();
            match controller.current_state() {
                GeState::Good => {
                    seen_good = true;
                    assert_eq!(
                        controller.loss_probability(),
                        0.001,
                        "Loss probability in Good state"
                    );
                }
                GeState::Bad => {
                    seen_bad = true;
                    assert_eq!(
                        controller.loss_probability(),
                        0.15,
                        "Loss probability in Bad state"
                    );
                }
            }

            if seen_good && seen_bad {
                break;
            }
        }

        assert!(seen_good, "Should have seen Good state");
        assert!(seen_bad, "Should have seen Bad state");
    }

    #[test]
    fn test_ge_no_transitions_with_zero_probabilities() {
        // Test that transitions don't occur with zero probabilities
        let params = GEParams {
            p_good: 0.05,
            p_bad: 0.2,
            p: 0.0, // No Good->Bad transitions
            r: 0.0, // No Bad->Good transitions
            seed: Some(777),
        };

        let mut controller = GEController::new(params);
        let initial_state = controller.current_state();

        // Call tick many times - state should never change
        for _ in 0..50 {
            let new_state = controller.tick();
            assert_eq!(
                new_state, initial_state,
                "State should not change with zero transition probabilities"
            );
        }
    }

    #[test]
    fn test_ge_parameter_updates() {
        let initial_params = GEParams {
            p_good: 0.01,
            p_bad: 0.1,
            p: 0.05,
            r: 0.1,
            seed: Some(555),
        };

        let mut controller = GEController::new(initial_params);
        assert_eq!(controller.loss_probability(), 0.01);

        // Update parameters
        let new_params = GEParams {
            p_good: 0.02, // Change Good loss probability
            p_bad: 0.3,   // Change Bad loss probability
            p: 0.8,       // High transition probability
            r: 0.9,
            seed: Some(555),
        };

        controller.update_params(new_params);

        // Loss probability should reflect new parameters
        if controller.current_state() == GeState::Good {
            assert_eq!(controller.loss_probability(), 0.02);
        }

        // Force transition to Bad state to test new bad loss probability
        for _ in 0..20 {
            controller.tick();
            if controller.current_state() == GeState::Bad {
                assert_eq!(controller.loss_probability(), 0.3);
                break;
            }
        }
    }
}
