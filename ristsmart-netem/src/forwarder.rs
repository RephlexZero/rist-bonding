//! UDP packet forwarder
//!
//! Forwards UDP packets from a namespace-local port to an external destination.

use crate::{errors::NetemError, ns::NetworkNamespace};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::info;
use tokio::task::JoinHandle;

type Result<T> = std::result::Result<T, NetemError>;

/// Configuration for UDP packet forwarding
#[derive(Debug, Clone)]
pub struct ForwarderConfig {
    pub src_port: u16,
    pub dst_host: String,
    pub dst_port: u16,
}

impl Default for ForwarderConfig {
    fn default() -> Self {
        Self {
            src_port: 5000,
            dst_host: "127.0.0.1".to_string(),
            dst_port: 6000,
        }
    }
}

/// UDP packet forwarder
#[derive(Debug)]
pub struct UdpForwarder {
    config: ForwarderConfig,
    handle: Option<JoinHandle<()>>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl UdpForwarder {
    pub fn new(config: ForwarderConfig) -> Self {
        Self {
            config,
            handle: None,
            shutdown_tx: None,
        }
    }

    pub async fn start(&mut self, ns: Arc<NetworkNamespace>) -> Result<()> {
        if self.handle.is_some() {
            return Ok(()); // Already started
        }

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        self.shutdown_tx = Some(shutdown_tx);

        let ns_clone = ns.clone();
        let config = self.config.clone();
        let handle = tokio::spawn(async move {
            if let Err(e) = run_forwarder_in_netns(ns_clone, config, shutdown_rx).await {
                tracing::error!("Forwarder error: {}", e);
            }
        });

        self.handle = Some(handle);

        info!(
            "Started forwarder: {} {} -> {}:{}",
            ns.ns_ip, self.config.src_port, self.config.dst_host, self.config.dst_port
        );

        Ok(())
    }

    pub async fn stop(&mut self) -> Result<()> {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }

        if let Some(handle) = self.handle.take() {
            let _ = handle.await;
        }

        Ok(())
    }

    pub fn is_running(&self) -> bool {
        self.handle.is_some()
    }
}

impl Drop for UdpForwarder {
    fn drop(&mut self) {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }
    }
}

/// Run the forwarder loop inside a network namespace
async fn run_forwarder_in_netns(
    ns: Arc<NetworkNamespace>,
    config: ForwarderConfig,
    mut shutdown_rx: tokio::sync::oneshot::Receiver<()>,
) -> Result<()> {
    // Create socket in the namespace
    let bind_addr = SocketAddr::from((ns.ns_ip, config.src_port));
    let socket = tokio::net::UdpSocket::bind(bind_addr).await
        .map_err(|e| NetemError::ForwarderBind(format!("Failed to bind socket: {}", e)))?;

    info!("UDP forwarder bound to {}:{}", ns.ns_ip, config.src_port);

    // Resolve destination
    let dst_addr = format!("{}:{}", config.dst_host, config.dst_port)
        .parse::<SocketAddr>()
        .map_err(|e| NetemError::ForwarderBind(format!("Invalid destination address: {}", e)))?;

    let mut buf = [0u8; 65536];

    loop {
        tokio::select! {
            // Check for shutdown signal
            _ = &mut shutdown_rx => {
                info!("UDP forwarder shutting down");
                break;
            }

            // Forward packets
            result = socket.recv_from(&mut buf) => {
                match result {
                    Ok((len, _src)) => {
                        if let Err(e) = socket.send_to(&buf[..len], dst_addr).await {
                            tracing::warn!("Failed to forward packet: {}", e);
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to receive packet: {}", e);
                    }
                }
            }
        }
    }

    Ok(())
}

/// Manager for multiple UDP forwarders
#[derive(Debug, Default)]
pub struct ForwarderManager {
    forwarders: HashMap<String, UdpForwarder>,
}

impl ForwarderManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn start_forwarder(
        &mut self,
        link_name: String,
        config: ForwarderConfig,
        ns: Arc<NetworkNamespace>,
    ) -> Result<()> {
        // Stop existing forwarder if any
        if let Some(existing) = self.forwarders.get_mut(&link_name) {
            existing.stop().await?;
        }

        let mut forwarder = UdpForwarder::new(config);
        forwarder.start(ns).await?;

        self.forwarders.insert(link_name, forwarder);
        Ok(())
    }

    pub async fn stop_forwarder(&mut self, link_name: &str) -> Result<()> {
        if let Some(mut forwarder) = self.forwarders.remove(link_name) {
            forwarder.stop().await?;
        }
        Ok(())
    }

    pub async fn stop_all(&mut self) -> Result<()> {
        for (_, mut forwarder) in self.forwarders.drain() {
            if let Err(e) = forwarder.stop().await {
                tracing::warn!("Error stopping forwarder: {}", e);
            }
        }
        Ok(())
    }

    pub fn is_running(&self, link_name: &str) -> bool {
        self.forwarders
            .get(link_name)
            .map(|f| f.is_running())
            .unwrap_or(false)
    }
}

impl Drop for ForwarderManager {
    fn drop(&mut self) {
        for (_, forwarder) in self.forwarders.iter_mut() {
            if let Some(shutdown_tx) = forwarder.shutdown_tx.take() {
                let _ = shutdown_tx.send(());
            }
        }
    }
}
