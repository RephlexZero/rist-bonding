//! Ornstein-Uhlenbeck process for throughput variation

use crate::errors::Result;
use crate::types::OUParams;
use rand_distr::{Distribution, Normal};
use std::time::Duration;
use tokio::time;
use tracing::{debug, warn};

/// Ornstein-Uhlenbeck process controller for throughput variation
#[derive(Debug)]
pub struct OUController {
    params: OUParams,
    current_value: f64,
    last_tick: std::time::Instant,
    rng: Option<rand::rngs::StdRng>,
}

impl OUController {
    pub fn new(params: OUParams) -> Result<Self> {
        use rand::SeedableRng;

        let rng = if let Some(seed) = params.seed {
            Some(rand::rngs::StdRng::seed_from_u64(seed))
        } else {
            None
        };

        Ok(Self {
            current_value: params.mean_bps as f64,
            params,
            last_tick: std::time::Instant::now(),
            rng,
        })
    }

    /// Update parameters (can be called from async context)
    pub fn update_params(&mut self, params: OUParams) {
        debug!(
            "Updating OU parameters: mean_bps={}, tau_ms={}",
            params.mean_bps, params.tau_ms
        );
        self.params = params;
    }

    /// Get current throughput value in bits per second
    pub fn current_bps(&self) -> u64 {
        self.current_value.max(0.0) as u64
    }

    /// Get current throughput value in bytes per second
    pub fn current_bytes_per_sec(&self) -> u64 {
        self.current_bps() / 8
    }

    /// Advance the OU process by one time step
    pub fn tick(&mut self) -> u64 {
        let now = std::time::Instant::now();
        let dt = now.duration_since(self.last_tick).as_secs_f64();
        self.last_tick = now;

        if dt <= 0.0 {
            return self.current_bps();
        }

        let tau_sec = self.params.tau_ms as f64 / 1000.0;
        let mean = self.params.mean_bps as f64;

        // OU process: dX = theta * (mu - X) * dt + sigma * dW
        // where theta = 1/tau, mu = mean, sigma = volatility
        let theta = 1.0 / tau_sec;
        let drift = theta * (mean - self.current_value) * dt;

        // Use seeded RNG if available, otherwise use thread_rng
        let noise = if let Some(ref mut rng) = self.rng {
            let normal = Normal::new(0.0, 1.0).unwrap_or_else(|_| Normal::new(0.0, 1.0).unwrap());
            self.params.sigma * mean * (2.0 * theta * dt).sqrt() * normal.sample(rng)
        } else {
            let mut thread_rng = rand::thread_rng();
            let normal = Normal::new(0.0, 1.0).unwrap_or_else(|_| Normal::new(0.0, 1.0).unwrap());
            self.params.sigma * mean * (2.0 * theta * dt).sqrt() * normal.sample(&mut thread_rng)
        };

        self.current_value += drift + noise;

        // Ensure value stays positive
        self.current_value = self.current_value.max(0.0);

        self.current_bps()
    }

    pub fn params(&self) -> &OUParams {
        &self.params
    }
}

/// Manager for OU controllers across multiple links
#[derive(Debug)]
pub struct OUManager {
    controllers: std::collections::HashMap<String, tokio::task::JoinHandle<Result<()>>>,
    shutdown_txs: std::collections::HashMap<String, tokio::sync::oneshot::Sender<()>>,
    update_rxs: std::collections::HashMap<String, tokio::sync::mpsc::UnboundedReceiver<u64>>,
}

impl Default for OUManager {
    fn default() -> Self {
        Self::new()
    }
}

impl OUManager {
    pub fn new() -> Self {
        Self {
            controllers: std::collections::HashMap::new(),
            shutdown_txs: std::collections::HashMap::new(),
            update_rxs: std::collections::HashMap::new(),
        }
    }

    pub async fn start_controller(
        &mut self,
        link_name: String,
        params: OUParams,
    ) -> Result<tokio::sync::mpsc::UnboundedReceiver<u64>> {
        // Stop existing controller if any
        self.stop_controller(&link_name).await?;

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (update_tx, update_rx) = tokio::sync::mpsc::unbounded_channel();

        let handle = tokio::spawn(async move {
            let (callback_tx, mut callback_rx) = tokio::sync::mpsc::unbounded_channel();

            // Spawn a separate task for the controller to avoid Send issues
            let controller_task = tokio::spawn(async move {
                spawn_ou_controller(params, 0, shutdown_rx, move |new_rate| {
                    let _ = callback_tx.send(new_rate);
                })
                .await
            });

            // Forward updates
            while let Some(rate) = callback_rx.recv().await {
                if update_tx.send(rate).is_err() {
                    break;
                }
            }

            let _ = controller_task.await;
            Ok(())
        });

        self.controllers.insert(link_name.clone(), handle);
        self.shutdown_txs.insert(link_name, shutdown_tx);

        Ok(update_rx)
    }

    pub async fn stop_controller(&mut self, link_name: &str) -> Result<()> {
        if let Some(shutdown_tx) = self.shutdown_txs.remove(link_name) {
            let _ = shutdown_tx.send(());
        }

        if let Some(handle) = self.controllers.remove(link_name) {
            if let Err(e) = handle.await {
                warn!("Error stopping OU controller for {}: {:?}", link_name, e);
            }
        }

        self.update_rxs.remove(link_name);

        Ok(())
    }

    pub async fn stop_all(&mut self) -> Result<()> {
        let link_names: Vec<_> = self.controllers.keys().cloned().collect();
        for link_name in link_names {
            if let Err(e) = self.stop_controller(&link_name).await {
                warn!("Error stopping OU controller for {}: {}", link_name, e);
            }
        }
        Ok(())
    }

    pub fn is_running(&self, link_name: &str) -> bool {
        self.controllers.contains_key(link_name)
    }
}

impl Drop for OUManager {
    fn drop(&mut self) {
        // Send shutdown signals to all controllers
        for (_, shutdown_tx) in self.shutdown_txs.drain() {
            let _ = shutdown_tx.send(());
        }
    }
}

/// Spawn an OU controller task
pub fn spawn_ou_controller<F>(
    params: OUParams,
    _seed: u64,
    shutdown_rx: tokio::sync::oneshot::Receiver<()>,
    callback: F,
) -> tokio::task::JoinHandle<()>
where
    F: Fn(u64) + Send + 'static,
{
    tokio::spawn(async move {
        let mut controller = OUController::new(params).unwrap();
        let tick_interval = Duration::from_millis(controller.params().tick_ms);
        let mut interval = time::interval(tick_interval);
        let mut shutdown_rx = shutdown_rx;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let new_rate = controller.tick();
                    callback(new_rate);
                }
                _ = &mut shutdown_rx => {
                    debug!("OU controller shutting down");
                    break;
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::OUParams;

    #[test]
    fn test_ou_controller_creation() {
        let params = OUParams::default();
        let controller = OUController::new(params).unwrap();
        assert!(controller.current_bps() > 0);
    }

    #[test]
    fn test_ou_tick() {
        let params = OUParams {
            mean_bps: 1_000_000, // 1 Mbps
            tau_ms: 1000,
            sigma: 0.1,
            tick_ms: 100,
            seed: None,
        };

        let mut controller = OUController::new(params).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1));

        let rate1 = controller.tick();
        let rate2 = controller.tick();

        // Both rates should be positive
        assert!(rate1 > 0);
        assert!(rate2 > 0);
    }
}
