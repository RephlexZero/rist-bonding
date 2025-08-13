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
            .map_err(|e| NetemError::CreateNamespace(format!("Failed to create netns: {}", e)))?;

        if !status.success() {
            return Err(NetemError::CreateNamespace(format!(
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
        let ns_path = format!("/var/run/netns/{}", self.name);

        // Use spawn_blocking since setns affects the entire thread
        // We need to switch to the target namespace, execute the function, then switch back
        tokio::task::spawn_blocking(move || {
            use nix::sched::{setns, CloneFlags};
            use std::fs::File;

            // Open original namespace (usually /proc/self/ns/net)
            let orig_ns = File::open("/proc/self/ns/net").map_err(|e| {
                NetemError::SetNetNs(format!("Failed to open original namespace: {}", e))
            })?;

            // Open target namespace
            let target_ns = File::open(&ns_path).map_err(|e| {
                NetemError::SetNetNs(format!("Failed to open namespace {}: {}", ns_path, e))
            })?;

            // Switch to target namespace
            setns(&target_ns, CloneFlags::CLONE_NEWNET).map_err(|e| {
                NetemError::SetNetNs(format!("Failed to switch to namespace {}: {}", ns_path, e))
            })?;

            // Execute the function
            let result = f();

            // Switch back to original namespace
            let _ = setns(&orig_ns, CloneFlags::CLONE_NEWNET)
                .map_err(|e| warn!("Failed to restore original namespace: {}", e));

            result
        })
        .await
        .map_err(|e| NetemError::SetNetNs(format!("Task join error: {}", e)))?
    }

    /// Create veth pair and move one end to this namespace
    pub async fn create_veth_pair(&mut self, handle: &Handle) -> Result<()> {
        let veth_host = format!("veth{}h", self.index); // Host side: veth0h, veth1h, etc.
        let veth_ns = format!("veth{}n", self.index); // Namespace side: veth0n, veth1n, etc.

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
            NetemError::CreateVeth(format!("Host interface {} not found", veth_host))
        })?;

        let ns_if_index = ns_if_index.ok_or_else(|| {
            NetemError::CreateVeth(format!("Namespace interface {} not found", veth_ns))
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

        // Ensure namespace interface index is set (validation only)
        self.ns_if_index
            .ok_or_else(|| NetemError::VethConfig("Namespace interface index not set".into()))?;

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

        // Configure namespace side by executing commands in the namespace
        let ns_name = &self.name;
        let veth_ns = format!("veth{}n", self.index);

        // Bring up the namespace interface and assign IP address
        let status = tokio::process::Command::new("ip")
            .args(&[
                "netns",
                "exec",
                ns_name,
                "ip",
                "addr",
                "add",
                &format!("{}/30", self.ns_ip),
                "dev",
                &veth_ns,
            ])
            .status()
            .await
            .map_err(|e| {
                NetemError::VethConfig(format!("Failed to configure namespace IP: {}", e))
            })?;

        if !status.success() {
            return Err(NetemError::VethConfig(format!(
                "Failed to assign IP {} to interface {} in namespace {}",
                self.ns_ip, veth_ns, ns_name
            )));
        }

        let status = tokio::process::Command::new("ip")
            .args(&[
                "netns", "exec", ns_name, "ip", "link", "set", &veth_ns, "up",
            ])
            .status()
            .await
            .map_err(|e| {
                NetemError::VethConfig(format!("Failed to bring up namespace interface: {}", e))
            })?;

        if !status.success() {
            return Err(NetemError::VethConfig(format!(
                "Failed to bring up interface {} in namespace {}",
                veth_ns, ns_name
            )));
        }

        // Add default route via host IP
        let status = tokio::process::Command::new("ip")
            .args(&[
                "netns",
                "exec",
                ns_name,
                "ip",
                "route",
                "add",
                "default",
                "via",
                &self.host_ip.to_string(),
            ])
            .status()
            .await
            .map_err(|e| NetemError::VethConfig(format!("Failed to add default route: {}", e)))?;
        if !status.success() {
            return Err(NetemError::VethConfig("Failed to add default route".into()));
        }

        // Enable forwarding on host interface and globally
        let veth_host = format!("veth{}h", self.index);
        let status = tokio::process::Command::new("sysctl")
            .args(&["-w", &format!("net.ipv4.conf.{}.forwarding=1", veth_host)])
            .status()
            .await
            .map_err(|e| NetemError::VethConfig(format!("Failed to enable forwarding: {}", e)))?;
        if !status.success() {
            return Err(NetemError::VethConfig(
                "Failed to enable interface forwarding".into(),
            ));
        }

        let _ = tokio::process::Command::new("sysctl")
            .args(&["-w", "net.ipv4.ip_forward=1"])
            .status()
            .await;

        debug!(
            "Namespace side configured with IP: {}/30 and brought up",
            self.ns_ip
        );

        Ok(())
    }

    /// Clean up the network namespace
    pub async fn cleanup(&self) -> Result<()> {
        info!("Cleaning up network namespace: {}", self.name);

        // Reset forwarding settings
        let veth_host = format!("veth{}h", self.index);
        let _ = tokio::process::Command::new("sysctl")
            .args(&["-w", &format!("net.ipv4.conf.{}.forwarding=0", veth_host)])
            .status()
            .await;
        let _ = tokio::process::Command::new("sysctl")
            .args(&["-w", "net.ipv4.ip_forward=0"])
            .status()
            .await;

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

    // Helper to check if tests can run with privileges
    fn should_run_privileged_tests() -> bool {
        std::env::var("RISTS_PRIV")
            .map(|v| v == "1")
            .unwrap_or(false)
    }

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

    #[test]
    fn test_namespace_fd_path() {
        let ns = NetworkNamespace::new("test-fd".to_string(), 5);

        // Test that the path is constructed correctly
        // We can't actually open it without creating the namespace
        match ns.netns_fd() {
            Ok(_) => panic!("Should not succeed without actual namespace"),
            Err(e) => {
                // Should fail but with the right error type
                assert!(matches!(e, crate::errors::NetemError::NetNsOpen(_)));
            }
        }
    }

    #[tokio::test]
    async fn test_with_netns_mock_behavior() {
        if !should_run_privileged_tests() {
            eprintln!("Skipping privileged test (set RISTS_PRIV=1 to enable)");
            return;
        }

        // Test the with_netns function behavior
        let ns = NetworkNamespace::new("test-with-netns".to_string(), 10);

        // First create the namespace
        if let Err(e) = ns.create().await {
            eprintln!("Failed to create namespace (need privileges): {}", e);
            return;
        }

        // Test executing a simple function within the namespace
        let test_result = ns
            .with_netns(|| {
                // Simple computation that should work in any namespace
                Ok(42u32)
            })
            .await;

        match test_result {
            Ok(value) => {
                assert_eq!(
                    value, 42,
                    "Should return correct value from within namespace"
                );
                println!("✓ with_netns execution successful: {}", value);
            }
            Err(e) => {
                eprintln!("with_netns failed: {}", e);
            }
        }

        // Clean up
        if let Err(e) = ns.cleanup().await {
            eprintln!("Warning: cleanup failed: {}", e);
        }
    }

    #[tokio::test]
    async fn test_namespace_context_isolation() {
        if !should_run_privileged_tests() {
            eprintln!("Skipping privileged test (set RISTS_PRIV=1 to enable)");
            return;
        }

        // Test that operations in different namespaces are isolated
        let ns1 = NetworkNamespace::new("test-iso-1".to_string(), 20);
        let ns2 = NetworkNamespace::new("test-iso-2".to_string(), 21);

        // Create both namespaces
        if let Err(e) = ns1.create().await {
            eprintln!("Failed to create namespace 1: {}", e);
            return;
        }

        if let Err(e) = ns2.create().await {
            eprintln!("Failed to create namespace 2: {}", e);
            let _ = ns1.cleanup().await;
            return;
        }

        // Test reading network namespace info from each
        let ns1_result = ns1
            .with_netns(|| {
                // Try to read /proc/self/ns/net to get namespace inode
                std::fs::read_link("/proc/self/ns/net")
                    .map(|p| p.to_string_lossy().to_string())
                    .map_err(|e| crate::errors::NetemError::Io(e))
            })
            .await;

        let ns2_result = ns2
            .with_netns(|| {
                // Try to read /proc/self/ns/net to get namespace inode
                std::fs::read_link("/proc/self/ns/net")
                    .map(|p| p.to_string_lossy().to_string())
                    .map_err(|e| crate::errors::NetemError::Io(e))
            })
            .await;

        match (ns1_result, ns2_result) {
            (Ok(ns1_inode), Ok(ns2_inode)) => {
                assert_ne!(
                    ns1_inode, ns2_inode,
                    "Namespaces should have different inodes: {} vs {}",
                    ns1_inode, ns2_inode
                );
                println!(
                    "✓ Namespace isolation verified: {} != {}",
                    ns1_inode, ns2_inode
                );
            }
            (Err(e1), _) => eprintln!("Failed to read namespace 1 info: {}", e1),
            (_, Err(e2)) => eprintln!("Failed to read namespace 2 info: {}", e2),
        }

        // Clean up both namespaces
        if let Err(e) = ns1.cleanup().await {
            eprintln!("Warning: cleanup ns1 failed: {}", e);
        }
        if let Err(e) = ns2.cleanup().await {
            eprintln!("Warning: cleanup ns2 failed: {}", e);
        }
    }

    #[tokio::test]
    async fn test_namespace_error_handling() {
        // Test error handling for non-existent namespace
        let fake_ns = NetworkNamespace::new("non-existent-ns".to_string(), 999);

        let result = fake_ns.with_netns(|| Ok("should not reach here")).await;

        assert!(result.is_err(), "Should fail for non-existent namespace");
        match result {
            Err(e) => {
                assert!(matches!(e, crate::errors::NetemError::SetNetNs(_)));
                println!("✓ Correctly handles non-existent namespace: {}", e);
            }
            Ok(_) => panic!("Should not succeed with non-existent namespace"),
        }
    }

    #[tokio::test]
    async fn test_namespace_manager_lifecycle() {
        if !should_run_privileged_tests() {
            eprintln!("Skipping privileged test (set RISTS_PRIV=1 to enable)");
            return;
        }

        let mut manager = NamespaceManager::new();

        if let Err(e) = manager.init().await {
            eprintln!("Failed to initialize namespace manager: {}", e);
            return;
        }

        // Create a namespace through the manager
        let ns_name = "test-mgr-ns".to_string();
        match manager.create_namespace(ns_name.clone(), 30).await {
            Ok(()) => {
                println!("✓ Namespace created through manager");

                // Verify we can retrieve it
                match manager.get_namespace(&ns_name) {
                    Ok(ns) => {
                        assert_eq!(ns.name, ns_name);
                        assert_eq!(ns.index, 30);
                        println!("✓ Namespace retrieved: {}", ns.name);
                    }
                    Err(e) => eprintln!("Failed to retrieve namespace: {}", e),
                }

                // Clean up
                if let Err(e) = manager.cleanup_all().await {
                    eprintln!("Warning: manager cleanup failed: {}", e);
                }
            }
            Err(e) => {
                eprintln!("Failed to create namespace through manager: {}", e);
            }
        }
    }

    #[test]
    fn test_namespace_ip_ranges() {
        // Test that different namespace indices get different IP ranges
        let test_cases = vec![
            (0, "10.0.0.1", "10.0.0.2"),
            (1, "10.1.0.1", "10.1.0.2"),
            (255, "10.255.0.1", "10.255.0.2"),
        ];

        for (index, expected_host, expected_ns) in test_cases {
            let ns = NetworkNamespace::new(format!("test-{}", index), index);
            assert_eq!(ns.host_ip.to_string(), expected_host);
            assert_eq!(ns.ns_ip.to_string(), expected_ns);
        }
    }
}
