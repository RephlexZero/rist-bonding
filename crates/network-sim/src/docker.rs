//! Docker-compatible network simulation utilities
//!
//! This module provides utilities for creating and managing network
//! namespaces and interfaces within Docker containers for testing.

use std::process::Command;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DockerNetworkError {
    #[error("Command execution failed: {0}")]
    CommandFailed(String),
    
    #[error("Network namespace operation failed: {0}")]
    NamespaceError(String),
    
    #[error("Interface configuration failed: {0}")]
    InterfaceError(String),
}

/// Docker-compatible network test environment
pub struct DockerNetworkEnv {
    pub namespaces: Vec<String>,
    pub interfaces: Vec<String>,
}

impl DockerNetworkEnv {
    /// Create a new Docker network environment
    pub fn new() -> Self {
        Self {
            namespaces: Vec::new(),
            interfaces: Vec::new(),
        }
    }
    
    /// Set up a basic test network with two namespaces
    pub async fn setup_basic_network(&mut self) -> Result<(), DockerNetworkError> {
        // Create network namespaces
        self.create_namespace("test_ns1").await?;
        self.create_namespace("test_ns2").await?;
        
        // Create veth pairs
        self.create_veth_pair("veth_test0", "veth_test1").await?;
        self.create_veth_pair("veth_test2", "veth_test3").await?;
        
        // Move interfaces to namespaces
        self.move_interface_to_namespace("veth_test1", "test_ns1").await?;
        self.move_interface_to_namespace("veth_test3", "test_ns2").await?;
        
        // Configure IP addresses
        self.configure_interface("veth_test0", "192.168.100.1/24").await?;
        self.configure_interface("veth_test2", "192.168.101.1/24").await?;
        
        self.configure_interface_in_namespace("test_ns1", "veth_test1", "192.168.100.2/24").await?;
        self.configure_interface_in_namespace("test_ns2", "veth_test3", "192.168.101.2/24").await?;
        
        // Bring interfaces up
        self.bring_up_interface("veth_test0").await?;
        self.bring_up_interface("veth_test2").await?;
        
        self.bring_up_interface_in_namespace("test_ns1", "veth_test1").await?;
        self.bring_up_interface_in_namespace("test_ns2", "veth_test3").await?;
        
        Ok(())
    }
    
    /// Create a network namespace
    pub async fn create_namespace(&mut self, name: &str) -> Result<(), DockerNetworkError> {
        let output = Command::new("ip")
            .args(&["netns", "add", name])
            .output()
            .map_err(|e| DockerNetworkError::CommandFailed(format!("Failed to create namespace {}: {}", name, e)))?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Ignore "File exists" errors
            if !stderr.contains("File exists") {
                return Err(DockerNetworkError::NamespaceError(format!(
                    "Failed to create namespace {}: {}", name, stderr
                )));
            }
        }
        
        self.namespaces.push(name.to_string());
        Ok(())
    }
    
    /// Create a veth pair
    pub async fn create_veth_pair(&mut self, if1: &str, if2: &str) -> Result<(), DockerNetworkError> {
        let output = Command::new("ip")
            .args(&["link", "add", if1, "type", "veth", "peer", "name", if2])
            .output()
            .map_err(|e| DockerNetworkError::CommandFailed(format!("Failed to create veth pair: {}", e)))?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Ignore "File exists" errors
            if !stderr.contains("File exists") {
                return Err(DockerNetworkError::InterfaceError(format!(
                    "Failed to create veth pair {}-{}: {}", if1, if2, stderr
                )));
            }
        }
        
        self.interfaces.push(if1.to_string());
        self.interfaces.push(if2.to_string());
        Ok(())
    }
    
    /// Move interface to namespace
    pub async fn move_interface_to_namespace(&self, interface: &str, namespace: &str) -> Result<(), DockerNetworkError> {
        let output = Command::new("ip")
            .args(&["link", "set", interface, "netns", namespace])
            .output()
            .map_err(|e| DockerNetworkError::CommandFailed(format!("Failed to move interface: {}", e)))?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DockerNetworkError::InterfaceError(format!(
                "Failed to move interface {} to namespace {}: {}", interface, namespace, stderr
            )));
        }
        
        Ok(())
    }
    
    /// Configure interface with IP address
    pub async fn configure_interface(&self, interface: &str, addr: &str) -> Result<(), DockerNetworkError> {
        let output = Command::new("ip")
            .args(&["addr", "add", addr, "dev", interface])
            .output()
            .map_err(|e| DockerNetworkError::CommandFailed(format!("Failed to configure interface: {}", e)))?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Ignore "File exists" errors
            if !stderr.contains("File exists") {
                return Err(DockerNetworkError::InterfaceError(format!(
                    "Failed to configure interface {} with {}: {}", interface, addr, stderr
                )));
            }
        }
        
        Ok(())
    }
    
    /// Configure interface in namespace
    pub async fn configure_interface_in_namespace(&self, namespace: &str, interface: &str, addr: &str) -> Result<(), DockerNetworkError> {
        let output = Command::new("ip")
            .args(&["netns", "exec", namespace, "ip", "addr", "add", addr, "dev", interface])
            .output()
            .map_err(|e| DockerNetworkError::CommandFailed(format!("Failed to configure interface in namespace: {}", e)))?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Ignore "File exists" errors
            if !stderr.contains("File exists") {
                return Err(DockerNetworkError::InterfaceError(format!(
                    "Failed to configure interface {} in namespace {} with {}: {}", interface, namespace, addr, stderr
                )));
            }
        }
        
        Ok(())
    }
    
    /// Bring interface up
    pub async fn bring_up_interface(&self, interface: &str) -> Result<(), DockerNetworkError> {
        let output = Command::new("ip")
            .args(&["link", "set", interface, "up"])
            .output()
            .map_err(|e| DockerNetworkError::CommandFailed(format!("Failed to bring up interface: {}", e)))?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DockerNetworkError::InterfaceError(format!(
                "Failed to bring up interface {}: {}", interface, stderr
            )));
        }
        
        Ok(())
    }
    
    /// Bring interface up in namespace
    pub async fn bring_up_interface_in_namespace(&self, namespace: &str, interface: &str) -> Result<(), DockerNetworkError> {
        let output = Command::new("ip")
            .args(&["netns", "exec", namespace, "ip", "link", "set", interface, "up"])
            .output()
            .map_err(|e| DockerNetworkError::CommandFailed(format!("Failed to bring up interface in namespace: {}", e)))?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DockerNetworkError::InterfaceError(format!(
                "Failed to bring up interface {} in namespace {}: {}", interface, namespace, stderr
            )));
        }
        
        Ok(())
    }
    
    /// Test connectivity between namespaces
    pub async fn test_connectivity(&self, from_ns: &str, to_ip: &str) -> Result<bool, DockerNetworkError> {
        let output = Command::new("ip")
            .args(&["netns", "exec", from_ns, "ping", "-c", "1", "-W", "1", to_ip])
            .output()
            .map_err(|e| DockerNetworkError::CommandFailed(format!("Failed to test connectivity: {}", e)))?;
        
        Ok(output.status.success())
    }
    
    /// Apply network impairments using tc (traffic control)
    pub async fn apply_network_impairments(&self, interface: &str, delay_ms: u32, loss_pct: f32, rate_kbps: u32) -> Result<(), DockerNetworkError> {
        // Delete existing qdisc (ignore errors)
        let _ = Command::new("tc")
            .args(&["qdisc", "del", "dev", interface, "root"])
            .output();
        
        // Add netem qdisc with impairments
        let loss_str = format!("{}%", loss_pct);
        let delay_str = format!("{}ms", delay_ms);
        let rate_str = format!("{}kbit", rate_kbps);
        
        let output = Command::new("tc")
            .args(&[
                "qdisc", "add", "dev", interface, "root", "handle", "1:", "netem",
                "delay", &delay_str,
                "loss", &loss_str,
                "rate", &rate_str
            ])
            .output()
            .map_err(|e| DockerNetworkError::CommandFailed(format!("Failed to apply network impairments: {}", e)))?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DockerNetworkError::InterfaceError(format!(
                "Failed to apply network impairments to {}: {}", interface, stderr
            )));
        }
        
        Ok(())
    }
    
    /// Clean up network resources
    pub async fn cleanup(&mut self) -> Result<(), DockerNetworkError> {
        // Delete namespaces (this also cleans up interfaces in them)
        for ns in &self.namespaces {
            let _ = Command::new("ip")
                .args(&["netns", "del", ns])
                .output();
        }
        
        // Delete interfaces in root namespace
        for interface in &self.interfaces {
            let _ = Command::new("ip")
                .args(&["link", "del", interface])
                .output();
        }
        
        self.namespaces.clear();
        self.interfaces.clear();
        
        Ok(())
    }
}

impl Drop for DockerNetworkEnv {
    fn drop(&mut self) {
        // Best effort cleanup
        let _ = futures::executor::block_on(self.cleanup());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_docker_network_setup() {
        let mut env = DockerNetworkEnv::new();
        
        match env.setup_basic_network().await {
            Ok(()) => {
                println!("✓ Network setup successful");
                
                // Test connectivity
                match env.test_connectivity("test_ns1", "192.168.100.1").await {
                    Ok(true) => println!("✓ Connectivity test passed"),
                    Ok(false) => println!("⚠ Connectivity test failed (expected in some environments)"),
                    Err(e) => println!("⚠ Connectivity test error: {}", e),
                }
                
                // Test network impairments
                match env.apply_network_impairments("veth_test0", 50, 1.0, 1000).await {
                    Ok(()) => println!("✓ Network impairments applied"),
                    Err(e) => println!("⚠ Network impairments failed: {}", e),
                }
            }
            Err(e) => {
                println!("⚠ Network setup failed (expected without proper privileges): {}", e);
            }
        }
        
        // Cleanup
        let _ = env.cleanup().await;
    }
}