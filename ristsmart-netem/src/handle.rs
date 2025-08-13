//! Main handles for controlling the emulator and links

use crate::errors::{NetemError, Result};
use crate::forwarder::{ForwarderConfig, ForwarderManager};
use crate::ge::{spawn_ge_controller, GEController};
use crate::metrics::{LinkMetrics, MetricsCollector, MetricsSnapshot};
use crate::ns::NetworkNamespace;
use crate::ou::{spawn_ou_controller, OUController};
use crate::qdisc::QdiscManager;
use crate::types::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, error, info, warn};

/// Handle for controlling the entire emulator
pub struct EmulatorHandle {
    links: Arc<RwLock<HashMap<String, Arc<LinkState>>>>,
    forwarder_manager: Arc<Mutex<ForwarderManager>>,
    metrics_collector: Arc<MetricsCollector>,
    seed: Option<u64>,
}

/// Internal state for a single link
pub struct LinkState {
    pub spec: LinkSpec,
    pub namespace: Arc<NetworkNamespace>,
    pub qdisc_manager: QdiscManager,
    pub ou_controller: Arc<RwLock<Option<OUController>>>,
    pub ge_controller: Arc<RwLock<Option<GEController>>>,
    pub ou_task: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
    pub ge_task: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
    pub ou_shutdown: Arc<RwLock<Option<tokio::sync::oneshot::Sender<()>>>>,
    pub ge_shutdown: Arc<RwLock<Option<tokio::sync::oneshot::Sender<()>>>>,
    pub current_rate: Arc<RwLock<u64>>,
    pub current_ge_state: Arc<RwLock<(GeState, f64)>>,
}

impl EmulatorHandle {
    /// Create a new emulator with the given links
    pub async fn new(link_specs: Vec<LinkSpec>, seed: Option<u64>) -> Result<Self> {
        let metrics_collector = Arc::new(MetricsCollector::new()?);
        let forwarder_manager = Arc::new(Mutex::new(ForwarderManager::new()));
        let links = Arc::new(RwLock::new(HashMap::new()));

        info!("Creating emulator with {} links", link_specs.len());

        // Clean up any leftover namespaces from previous runs
        Self::cleanup_leftover_namespaces(&link_specs).await?;

        // Create namespaces and initial setup
        {
            let mut links_write = links.write().await;

            // Create rtnetlink handle for veth management
            let (connection, handle, _) = rtnetlink::new_connection()?;
            tokio::spawn(connection);

            for (index, spec) in link_specs.into_iter().enumerate() {
                let namespace_name = format!("lnk-{}", index);

                // Create network namespace
                let mut namespace = NetworkNamespace::new(namespace_name, index as u32);
                namespace.create().await?;
                
                // Create veth pair and configure addresses
                namespace.create_veth_pair(&handle).await?;
                namespace.configure_addresses(&handle).await?;
                
                let namespace = Arc::new(namespace);

                // Create qdisc manager with proper interface index
                let qdisc_manager = QdiscManager::new(
                    namespace.ns_if_index.unwrap_or(0), 
                    format!("lnk-{}", index),
                    format!("veth{}n", index)
                );

                // Create controllers (not started yet)
                let ou_controller = Arc::new(RwLock::new(None));
                let ge_controller = Arc::new(RwLock::new(None));

                let link_state = Arc::new(LinkState {
                    spec: spec.clone(),
                    namespace,
                    qdisc_manager,
                    ou_controller,
                    ge_controller,
                    ou_task: Arc::new(RwLock::new(None)),
                    ge_task: Arc::new(RwLock::new(None)),
                    ou_shutdown: Arc::new(RwLock::new(None)),
                    ge_shutdown: Arc::new(RwLock::new(None)),
                    current_rate: Arc::new(RwLock::new(spec.ou.mean_bps)),
                    current_ge_state: Arc::new(RwLock::new((GeState::Good, spec.ge.p_good))),
                });

                links_write.insert(spec.name.clone(), link_state);
                info!("Created link: {}", spec.name);
            }
        }

        Ok(Self {
            links,
            forwarder_manager,
            metrics_collector,
            seed,
        })
    }

    /// Start all controllers and apply initial qdisc configuration
    pub async fn start(&self) -> Result<()> {
        info!("Starting emulator");

        let links_read = self.links.read().await;

        for (name, link_state) in links_read.iter() {
            info!("Starting link: {}", name);

            // Setup qdiscs with initial parameters
            let initial_rate = *link_state.current_rate.read().await;
            let initial_ge_state = link_state.current_ge_state.read().await.0;

            // Run qdisc setup in the correct namespace
            let ns = link_state.namespace.clone();
            let spec = link_state.spec.clone();

            ns.with_netns(|| {
                // This will be executed in the namespace context
                Ok(())
            })
            .await?;

            // For now, we'll use the tc commands which handle namespace context
            link_state
                .qdisc_manager
                .setup_qdiscs(
                    &spec.rate_limiter,
                    initial_rate,
                    &spec.delay,
                    &spec.ge,
                    initial_ge_state,
                )
                .await?;

            // Start OU controller
            {
                let ou_params = spec.ou.clone();
                let qdisc_manager = link_state.qdisc_manager.clone();
                let rate_limiter = spec.rate_limiter.clone();
                let current_rate = link_state.current_rate.clone();

                let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
                *link_state.ou_shutdown.write().await = Some(shutdown_tx);

                let ou_task = spawn_ou_controller(ou_params, 0, shutdown_rx, move |new_rate| {
                    let qdisc_manager = qdisc_manager.clone();
                    let rate_limiter = rate_limiter.clone();
                    let current_rate = current_rate.clone();

                    tokio::spawn(async move {
                        if let Err(e) = qdisc_manager.update_rate(&rate_limiter, new_rate).await {
                            error!("Failed to update rate: {}", e);
                        } else {
                            *current_rate.write().await = new_rate;
                        }
                    });
                });

                *link_state.ou_task.write().await = Some(ou_task);
            }

            // Start GE controller
            {
                let ge_params = spec.ge.clone();
                let qdisc_manager = link_state.qdisc_manager.clone();
                let delay_profile = spec.delay.clone();
                let current_ge_state = link_state.current_ge_state.clone();

                let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
                *link_state.ge_shutdown.write().await = Some(shutdown_tx);

                let ge_task = spawn_ge_controller(
                    ge_params.clone(),
                    self.seed,
                    100, // Tick every 100ms
                    shutdown_rx,
                    move |state, loss_prob| {
                        let qdisc_manager = qdisc_manager.clone();
                        let ge_params = ge_params.clone();
                        let delay_profile = delay_profile.clone();
                        let current_ge_state = current_ge_state.clone();

                        tokio::spawn(async move {
                            if let Err(e) = qdisc_manager
                                .update_netem(&delay_profile, &ge_params, state)
                                .await
                            {
                                error!("Failed to update netem: {}", e);
                            } else {
                                *current_ge_state.write().await = (state, loss_prob);
                            }
                        });

                        Ok(())
                    },
                );

                *link_state.ge_task.write().await = Some(ge_task);
            }
        }

        info!("Emulator started successfully");
        Ok(())
    }

    /// Stop all controllers (but keep qdiscs active)
    pub async fn stop(&self) -> Result<()> {
        info!("Stopping emulator");

        let links_read = self.links.read().await;

        for (name, link_state) in links_read.iter() {
            info!("Stopping link: {}", name);

            // Stop OU controller
            if let Some(shutdown_tx) = link_state.ou_shutdown.write().await.take() {
                let _ = shutdown_tx.send(());
            }
            if let Some(task) = link_state.ou_task.write().await.take() {
                task.abort();
                let _ = task.await; // Ignore cancellation errors
            }

            // Stop GE controller
            if let Some(shutdown_tx) = link_state.ge_shutdown.write().await.take() {
                let _ = shutdown_tx.send(());
            }
            if let Some(task) = link_state.ge_task.write().await.take() {
                task.abort();
                let _ = task.await; // Ignore cancellation errors
            }
        }

        // Stop all forwarders
        self.forwarder_manager.lock().await.stop_all().await?;

        info!("Emulator stopped");
        Ok(())
    }

    /// Teardown everything (namespaces, qdiscs, etc.)
    pub async fn teardown(self) -> Result<()> {
        info!("Tearing down emulator");

        // Stop controllers first
        self.stop().await?;

        let links_read = self.links.read().await;

        for (name, link_state) in links_read.iter() {
            info!("Cleaning up link: {}", name);

            // Cleanup qdiscs
            if let Err(e) = link_state.qdisc_manager.cleanup().await {
                warn!("Failed to cleanup qdiscs for {}: {}", name, e);
            }

            // Cleanup namespace
            if let Err(e) = link_state.namespace.cleanup().await {
                warn!("Failed to cleanup namespace for {}: {}", name, e);
            }
        }

        info!("Emulator teardown complete");
        Ok(())
    }

    /// Get handle for a specific link
    pub fn link(&self, name: &str) -> Option<LinkHandle> {
        // We can't block in a sync function, so we'll return a handle that can access the link
        Some(LinkHandle {
            name: name.to_string(),
            emulator: self.links.clone(),
            forwarder_manager: self.forwarder_manager.clone(),
        })
    }

    /// Get current metrics snapshot
    pub async fn metrics(&self) -> Result<MetricsSnapshot> {
        let mut snapshot = MetricsSnapshot::new();
        let links_read = self.links.read().await;

        for (name, link_state) in links_read.iter() {
            let current_rate = *link_state.current_rate.read().await;
            let (ge_state, loss_pct) = *link_state.current_ge_state.read().await;

            // Get interface stats
            let (tx_bytes, rx_bytes, tx_packets, rx_packets, dropped) = self
                .metrics_collector
                .collect_interface_stats(link_state.namespace.ns_if_index)
                .await
                .unwrap_or((0, 0, 0, 0, 0));

            let link_metrics = LinkMetrics {
                namespace: name.clone(),
                egress_rate_bps: current_rate,
                ge_state,
                loss_pct: loss_pct * 100.0, // Convert to percentage
                delay_ms: link_state.spec.delay.delay_ms,
                jitter_ms: link_state.spec.delay.jitter_ms,
                tx_bytes,
                rx_bytes,
                tx_packets,
                rx_packets,
                dropped_packets: dropped,
            };

            snapshot.add_link(link_metrics);
        }

        Ok(snapshot)
    }

    /// Clean up any leftover namespaces from previous runs
    async fn cleanup_leftover_namespaces(link_specs: &[LinkSpec]) -> Result<()> {
        info!("Cleaning up leftover namespaces from previous runs");

        for (index, _spec) in link_specs.iter().enumerate() {
            let namespace_name = format!("lnk-{}", index);
            
            // Try to delete the namespace, ignore errors if it doesn't exist
            let status = tokio::process::Command::new("ip")
                .args(&["netns", "delete", &namespace_name])
                .status()
                .await;

            match status {
                Ok(status) if status.success() => {
                    info!("Cleaned up leftover namespace: {}", namespace_name);
                }
                Ok(_) => {
                    // Namespace didn't exist or couldn't be deleted, that's fine
                    debug!("Namespace {} doesn't exist or already cleaned up", namespace_name);
                }
                Err(e) => {
                    warn!("Failed to check/cleanup namespace {}: {}", namespace_name, e);
                }
            }
        }

        Ok(())
    }

    /// Clean up all resources used by this emulator
    pub async fn cleanup(&self) -> Result<()> {
        info!("Cleaning up emulator resources");

        // Stop all forwarders
        {
            let mut forwarder_manager = self.forwarder_manager.lock().await;
            forwarder_manager.stop_all().await?;
        }

        // Clean up all links
        let mut links_write = self.links.write().await;
        for (name, link_state) in links_write.drain() {
            info!("Cleaning up link: {}", name);

            // Stop controllers
            if let Some(sender) = link_state.ou_shutdown.write().await.take() {
                let _ = sender.send(());
            }
            if let Some(sender) = link_state.ge_shutdown.write().await.take() {
                let _ = sender.send(());
            }

            // Wait for tasks to finish
            if let Some(task) = link_state.ou_task.write().await.take() {
                let _ = task.await;
            }
            if let Some(task) = link_state.ge_task.write().await.take() {
                let _ = task.await;
            }

            // Clean up namespace
            if let Err(e) = link_state.namespace.cleanup().await {
                warn!("Failed to clean up namespace {}: {}", link_state.namespace.name, e);
            }
        }

        info!("Emulator cleanup complete");
        Ok(())
    }
}

impl Drop for EmulatorHandle {
    fn drop(&mut self) {
        // Best effort cleanup - spawn a blocking task since we can't await in Drop
        if !self.links.try_read().map_or(false, |links| links.is_empty()) {
            warn!("EmulatorHandle dropped without cleanup - attempting best effort cleanup");
            
            // Clone Arc handles for the cleanup task
            let links = self.links.clone();
            let forwarder_manager = self.forwarder_manager.clone();
            
            // Spawn cleanup on a separate thread since we can't await in Drop
            std::thread::spawn(move || {
                let rt = match tokio::runtime::Runtime::new() {
                    Ok(rt) => rt,
                    Err(e) => {
                        eprintln!("Warning: Failed to create runtime for cleanup: {}", e);
                        return;
                    }
                };
                
                rt.block_on(async move {
                    // Stop forwarders
                    if let Ok(mut fm) = forwarder_manager.try_lock() {
                        if let Err(e) = fm.stop_all().await {
                            eprintln!("Warning: Failed to stop forwarders during drop: {}", e);
                        }
                    }
                    
                    // Clean up namespaces
                    if let Ok(mut links_write) = links.try_write() {
                        for (name, link_state) in links_write.drain() {
                            if let Err(e) = link_state.namespace.cleanup().await {
                                eprintln!("Warning: Failed to cleanup namespace {} during drop: {}", name, e);
                            }
                        }
                    }
                });
            });
        }
    }
}

/// Handle for controlling a specific link
pub struct LinkHandle {
    name: String,
    emulator: Arc<RwLock<HashMap<String, Arc<LinkState>>>>,
    forwarder_manager: Arc<Mutex<ForwarderManager>>,
}

impl LinkHandle {
    /// Update OU parameters for this link
    pub async fn set_ou(&self, ou: OUParams) -> Result<()> {
        let links = self.emulator.read().await;
        let link_state = links
            .get(&self.name)
            .ok_or_else(|| NetemError::LinkNotFound(self.name.clone()))?;

        // Update the controller if it's running
        if let Some(controller) = link_state.ou_controller.write().await.as_mut() {
            controller.update_params(ou);
        }

        Ok(())
    }

    /// Update GE parameters for this link
    pub async fn set_ge(&self, ge: GEParams) -> Result<()> {
        let links = self.emulator.read().await;
        let link_state = links
            .get(&self.name)
            .ok_or_else(|| NetemError::LinkNotFound(self.name.clone()))?;

        // Update the controller if it's running
        if let Some(controller) = link_state.ge_controller.write().await.as_mut() {
            controller.update_params(ge);
        }

        Ok(())
    }

    /// Update delay profile for this link
    pub async fn set_delay(&self, delay: DelayProfile) -> Result<()> {
        let links = self.emulator.read().await;
        let link_state = links
            .get(&self.name)
            .ok_or_else(|| NetemError::LinkNotFound(self.name.clone()))?;

        // Update netem configuration
        let current_ge_state = link_state.current_ge_state.read().await;
        link_state
            .qdisc_manager
            .update_netem(&delay, &link_state.spec.ge, current_ge_state.0)
            .await?;

        Ok(())
    }

    /// Bind UDP forwarder to this link
    pub async fn bind_forwarder(&self, src_port: u16, dst_host: &str, dst_port: u16) -> Result<()> {
        let links = self.emulator.read().await;
        let link_state = links
            .get(&self.name)
            .ok_or_else(|| NetemError::LinkNotFound(self.name.clone()))?;

        let config = ForwarderConfig {
            src_port,
            dst_host: dst_host.to_string(),
            dst_port,
        };

        self.forwarder_manager
            .lock()
            .await
            .bind_forwarder(&self.name, link_state.namespace.clone(), config)
            .await?;

        Ok(())
    }

    /// Unbind UDP forwarder from this link
    pub async fn unbind_forwarder(&self) -> Result<()> {
        self.forwarder_manager
            .lock()
            .await
            .unbind_forwarder(&self.name)
            .await
    }
}

impl Clone for LinkHandle {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            emulator: self.emulator.clone(),
            forwarder_manager: self.forwarder_manager.clone(),
        }
    }
}
