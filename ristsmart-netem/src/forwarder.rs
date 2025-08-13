//! UDP packet forwarder
//!
//! Forwards UDP packets from a namespace-local port to an external destination.

use crate::{errors::NetemError, ns::NetworkNamespace};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tracing::info;

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
    // Clone necessary data before moving into closure
    let ns_ip = ns.ns_ip;
    let ns_clone = ns.clone();

    // Execute socket creation within the network namespace
    let socket = ns_clone
        .with_netns(move || {
            // Create socket in the namespace
            let bind_addr = SocketAddr::from((ns_ip, config.src_port));

            // We need to use std socket creation since we're in a blocking context
            let socket = std::net::UdpSocket::bind(bind_addr)
                .map_err(|e| NetemError::ForwarderBind(format!("Failed to bind socket: {}", e)))?;

            // Convert to tokio socket
            socket.set_nonblocking(true).map_err(|e| {
                NetemError::ForwarderBind(format!("Failed to set nonblocking: {}", e))
            })?;

            tokio::net::UdpSocket::from_std(socket)
                .map_err(|e| NetemError::ForwarderBind(format!("Failed to convert socket: {}", e)))
        })
        .await?;

    info!("UDP forwarder bound to {}:{}", ns.ns_ip, config.src_port);

    // Resolve destination
    let dst_addr = format!("{}:{}", config.dst_host, config.dst_port)
        .parse::<SocketAddr>()
        .map_err(|e| NetemError::ForwarderBind(format!("Invalid destination address: {}", e)))?;

    let mut buf = [0u8; 65536];
    // Track last namespace source to enable replies
    let mut last_ns_addr: Option<SocketAddr> = None;

    loop {
        tokio::select! {
            // Check for shutdown signal
            _ = &mut shutdown_rx => {
                info!("UDP forwarder shutting down");
                break;
            }

            // Forward packets in both directions
            result = socket.recv_from(&mut buf) => {
                match result {
                    Ok((len, src)) => {
                        if src == dst_addr {
                            // Packet from destination -> namespace
                            if let Some(ns_addr) = last_ns_addr {
                                if let Err(e) = socket.send_to(&buf[..len], ns_addr).await {
                                    tracing::warn!("Failed to forward packet to namespace: {}", e);
                                }
                            }
                        } else {
                            // Packet from namespace -> destination
                            last_ns_addr = Some(src);
                            if let Err(e) = socket.send_to(&buf[..len], dst_addr).await {
                                tracing::warn!("Failed to forward packet to destination: {}", e);
                            }
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

    /// Start a forwarder (alias for backwards compatibility)
    pub async fn bind_forwarder(
        &mut self,
        link_name: &str,
        ns: Arc<NetworkNamespace>,
        config: ForwarderConfig,
    ) -> Result<()> {
        self.start_forwarder(link_name.to_string(), config, ns)
            .await
    }

    /// Stop a forwarder (alias for backwards compatibility)  
    pub async fn unbind_forwarder(&mut self, link_name: &str) -> Result<()> {
        self.stop_forwarder(link_name).await
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ns::NetworkNamespace;
    use std::net::SocketAddr;
    use std::sync::Arc;

    fn should_run_privileged_tests() -> bool {
        std::env::var("RISTS_PRIV")
            .map(|v| v == "1")
            .unwrap_or(false)
    }

    #[tokio::test]
    async fn test_bidirectional_forwarder() {
        if !should_run_privileged_tests() {
            eprintln!("Skipping privileged test (set RISTS_PRIV=1 to enable)");
            return;
        }

        let mut ns_obj = NetworkNamespace::new("test-fwd".to_string(), 40);
        if let Err(e) = ns_obj.create().await {
            eprintln!("Skipping test: failed to create namespace: {}", e);
            return;
        }
        let (connection, handle, _) = match rtnetlink::new_connection() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Skipping test: failed to create netlink connection: {}", e);
                return;
            }
        };
        tokio::spawn(connection);
        if let Err(e) = ns_obj.create_veth_pair(&handle).await {
            eprintln!("Skipping test: failed to create veth pair: {}", e);
            return;
        }
        if let Err(e) = ns_obj.configure_addresses(&handle).await {
            eprintln!("Skipping test: failed to configure addresses: {}", e);
            return;
        }
        let ns = Arc::new(ns_obj);

        // Echo server on host side
        let host_ip = ns.host_ip;
        let echo = tokio::spawn(async move {
            let sock = tokio::net::UdpSocket::bind((host_ip, 6001)).await.unwrap();
            let mut buf = [0u8; 64];
            if let Ok((len, addr)) = sock.recv_from(&mut buf).await {
                let _ = sock.send_to(&buf[..len], addr).await;
            }
        });

        // Start forwarder in namespace
        let mut fwd = UdpForwarder::new(ForwarderConfig {
            src_port: 5001,
            dst_host: host_ip.to_string(),
            dst_port: 6001,
        });
        fwd.start(ns.clone()).await.unwrap();

        // Client inside namespace sending to forwarder
        let ns_clone = ns.clone();
        let socket = ns
            .with_netns(move || {
                let sock = std::net::UdpSocket::bind((ns_clone.ns_ip, 0)).unwrap();
                sock.set_nonblocking(true).unwrap();
                Ok(tokio::net::UdpSocket::from_std(sock).unwrap())
            })
            .await
            .unwrap();

        let forwarder_addr = SocketAddr::from((ns.ns_ip, 5001));
        socket.send_to(b"ping", forwarder_addr).await.unwrap();

        let mut buf = [0u8; 64];
        let (len, _) = socket.recv_from(&mut buf).await.unwrap();
        assert_eq!(&buf[..len], b"ping");

        fwd.stop().await.unwrap();
        let _ = echo.await;
        ns.cleanup().await.unwrap();
    }
}
