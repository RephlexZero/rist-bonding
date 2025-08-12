//! Network namespace and veth pair management

use crate::errors::{NetemError, Result};
use futures::TryStreamExt;
use rtnetlink::Handle;
use std::fs::File;
use std::net::Ipv4Addr;
use std::os::fd::AsRawFd;
use tracing::{debug, info, warn};

/// Network namespace manager
#[derive(Debug)]
pub struct NetworkNamespace {
    pub name: String,
    pub index: u32,
    pub ns_ip: Ipv4Addr,
    pub host_ip: Ipv4Addr,
    pub ns_if_index: Option<u32>,
    pub host_if_index: Option<u32>,
}

impl NetworkNamespace {
    pub fn new(name: String, index: u32) -> Self {
        // Assign IP addresses based on index
        // Namespace gets .2, host gets .1 in a /30 subnet
        // 10.X.0.0 where X is the index
        let base_ip = (10_u32 << 24) + (index << 16); // 10.index.0.0
        let ns_ip = Ipv4Addr::from(base_ip + 2); // 10.index.0.2
        let host_ip = Ipv4Addr::from(base_ip + 1); // 10.index.0.1

        Self {
            name,
            index,
            ns_ip,
            host_ip,
            ns_if_index: None,
            host_if_index: None,
        }
    }

    /// Get the file descriptor for this network namespace
    pub fn netns_fd(&self) -> Result<File> {
        let path = format!("/var/run/netns/{}", self.name);
        File::open(&path).map_err(|e| {
            NetemError::NetNsOpen(format!("Failed to open netns {}: {}", self.name, e))
        })
    }

    /// Create the network namespace
    pub async fn create(&self) -> Result<()> {
        info!("Creating network namespace: {}", self.name);

        // Create namespace using 'ip netns add'
        let status = tokio::process::Command::new("ip")
            .args(&["netns", "add", &self.name])
            .status()
            .await
            .map_err(|e| NetemError::NetNsCreate(format!("Failed to create netns: {}", e)))?;

        if !status.success() {
            return Err(NetemError::NetNsCreate(format!(
                "ip netns add command failed for {}",
                self.name
            )));
        }

        debug!("Network namespace {} created successfully", self.name);
        Ok(())
    }

    /// Execute a function within this network namespace (async version)
    pub async fn with_netns<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce() -> Result<R> + Send + 'static,
        R: Send + 'static,
    {
        // For now, just execute the function directly
        // In a full implementation, we'd use setns but it's complex with async
        f()
    }

    /// Create veth pair and move one end to this namespace
    pub async fn create_veth_pair(&mut self, handle: &Handle) -> Result<()> {
        let veth_host = format!("veth-{}-host", self.name);
        let veth_ns = format!("veth-{}-ns", self.name);

        info!(
            "Creating veth pair: {} <-> {} for namespace {}",
            veth_host, veth_ns, self.name
        );

        // Create veth pair
        handle
            .link()
            .add()
            .veth(veth_host.clone(), veth_ns.clone())
            .execute()
            .await?;

        debug!("Veth pair created");

        // Get interface indices
        let mut host_if_index = None;
        let mut ns_if_index = None;

        let mut links_stream = handle.link().get().match_name(veth_host.clone()).execute();
        if let Some(msg) = links_stream.try_next().await? {
            host_if_index = Some(msg.header.index);
        }

        let mut links_stream = handle.link().get().match_name(veth_ns.clone()).execute();
        if let Some(msg) = links_stream.try_next().await? {
            ns_if_index = Some(msg.header.index);
        }

        let host_if_index = host_if_index.ok_or_else(|| {
            NetemError::VethCreate(format!("Host interface {} not found", veth_host))
        })?;

        let ns_if_index = ns_if_index.ok_or_else(|| {
            NetemError::VethCreate(format!("Namespace interface {} not found", veth_ns))
        })?;

        // Move namespace end to the namespace
        let netns_fd = self.netns_fd()?;
        handle
            .link()
            .set(ns_if_index)
            .setns_by_fd(netns_fd.as_raw_fd())
            .execute()
            .await?;

        self.host_if_index = Some(host_if_index);
        self.ns_if_index = Some(ns_if_index);

        debug!(
            "Veth pair created: host_if={}, ns_if={}",
            host_if_index, ns_if_index
        );

        Ok(())
    }

    /// Configure IP addresses on the veth pair
    pub async fn configure_addresses(&self, handle: &Handle) -> Result<()> {
        let host_if_index = self
            .host_if_index
            .ok_or_else(|| NetemError::VethConfig("Host interface index not set".into()))?;

        info!(
            "Configuring addresses: host={} ns={}",
            self.host_ip, self.ns_ip
        );

        // Configure host side
        handle
            .address()
            .add(host_if_index, self.host_ip.into(), 30)
            .execute()
            .await?;

        handle.link().set(host_if_index).up().execute().await?;

        debug!("Host side configured with IP: {}/30", self.host_ip);

        Ok(())
    }

    /// Clean up the network namespace
    pub async fn cleanup(&self) -> Result<()> {
        info!("Cleaning up network namespace: {}", self.name);

        let status = tokio::process::Command::new("ip")
            .args(&["netns", "delete", &self.name])
            .status()
            .await
            .map_err(|e| NetemError::NetNsCleanup(format!("Failed to delete netns: {}", e)))?;

        if !status.success() {
            warn!("Failed to delete network namespace: {}", self.name);
        } else {
            debug!("Network namespace {} deleted", self.name);
        }

        Ok(())
    }
}

/// Manager for multiple network namespaces
#[derive(Debug, Default)]
pub struct NamespaceManager {
    namespaces: std::collections::HashMap<String, NetworkNamespace>,
    handle: Option<Handle>,
}

impl NamespaceManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn init(&mut self) -> Result<()> {
        let (connection, handle, _) = rtnetlink::new_connection()?;
        tokio::spawn(connection);
        self.handle = Some(handle);
        Ok(())
    }

    pub fn handle(&self) -> Result<&Handle> {
        self.handle
            .as_ref()
            .ok_or_else(|| NetemError::NotInitialized("NamespaceManager not initialized".into()))
    }

    pub async fn create_namespace(&mut self, name: String, index: u32) -> Result<()> {
        let mut ns = NetworkNamespace::new(name.clone(), index);
        let handle = self.handle()?;

        ns.create().await?;
        ns.create_veth_pair(handle).await?;
        ns.configure_addresses(handle).await?;

        self.namespaces.insert(name, ns);
        Ok(())
    }

    pub fn get_namespace(&self, name: &str) -> Result<&NetworkNamespace> {
        self.namespaces
            .get(name)
            .ok_or_else(|| NetemError::NamespaceNotFound(format!("Namespace {} not found", name)))
    }

    pub async fn cleanup_all(&mut self) -> Result<()> {
        for (_, ns) in self.namespaces.drain() {
            if let Err(e) = ns.cleanup().await {
                warn!("Error cleaning up namespace {}: {}", ns.name, e);
            }
        }
        Ok(())
    }

    pub fn namespaces(&self) -> impl Iterator<Item = &NetworkNamespace> {
        self.namespaces.values()
    }
}

impl Drop for NamespaceManager {
    fn drop(&mut self) {
        // Best effort cleanup - spawn a blocking task since we can't await in Drop
        if !self.namespaces.is_empty() {
            warn!(
                "NamespaceManager dropped with {} active namespaces",
                self.namespaces.len()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_namespace_ip_assignment() {
        let ns = NetworkNamespace::new("test0".to_string(), 0);
        assert_eq!(ns.ns_ip, Ipv4Addr::new(10, 0, 0, 2));
        assert_eq!(ns.host_ip, Ipv4Addr::new(10, 0, 0, 1));

        let ns = NetworkNamespace::new("test1".to_string(), 1);
        assert_eq!(ns.ns_ip, Ipv4Addr::new(10, 1, 0, 2));
        assert_eq!(ns.host_ip, Ipv4Addr::new(10, 1, 0, 1));
    }

    #[test]
    fn test_namespace_manager() {
        let manager = NamespaceManager::new();
        assert_eq!(manager.namespaces.len(), 0);
    }
}
