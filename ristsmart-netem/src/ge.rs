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
}

impl GEController {
    pub fn new(params: GEParams) -> Self {
        Self {
            params,
            state: GeState::Good,
            last_tick: std::time::Instant::now(),
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

        let mut rng = rand::thread_rng();
        let rand_val: f64 = rng.gen();

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
        let mut rng = rand::thread_rng();
        let rand_val: f64 = rng.gen();
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
    _seed: Option<u64>,
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
        };
        let controller = GEController::new(params);
        assert_eq!(controller.loss_probability(), 0.01);
    }
}
