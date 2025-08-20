//! Network namespace management
//!
//! This module provides functionality to create, delete, and enter Linux
//! network namespaces using the `/var/run/netns/<name>` convention.

use nix::mount::{mount, umount, MsFlags};
use nix::sched::{setns, CloneFlags};
use nix::unistd::getpid;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::os::unix::io::{AsRawFd, RawFd};
use std::path::PathBuf;
use thiserror::Error;
use tokio::fs;
use tracing::{debug, info, warn};

#[derive(Error, Debug)]
pub enum NetNsError {
    #[error("Failed to create netns directory: {0}")]
    CreateDir(std::io::Error),
    
    #[error("Failed to create netns file: {0}")]
    CreateFile(std::io::Error),
    
    #[error("Failed to mount namespace: {0}")]
    Mount(nix::Error),
    
    #[error("Failed to enter namespace: {0}")]
    SetNs(nix::Error),
    
    #[error("Failed to open namespace file: {0}")]
    OpenNs(std::io::Error),
    
    #[error("Namespace '{0}' not found")]
    NotFound(String),
    
    #[error("Namespace '{0}' already exists")]
    AlreadyExists(String),
    
    #[error("Insufficient permissions (CAP_NET_ADMIN required)")]
    Permission,
}

/// Network namespace manager
pub struct Manager {
    /// Map of namespace name to file descriptor
    namespaces: HashMap<String, File>,
    /// Base directory for namespace files
    base_dir: PathBuf,
}

impl Manager {
    /// Create a new namespace manager
    pub fn new() -> Result<Self, NetNsError> {
        let base_dir = PathBuf::from("/var/run/netns");
        
        // Ensure the base directory exists
        std::fs::create_dir_all(&base_dir)
            .map_err(NetNsError::CreateDir)?;
            
        Ok(Self {
            namespaces: HashMap::new(),
            base_dir,
        })
    }

    /// Create a new network namespace
    pub async fn create_namespace(&mut self, name: &str) -> Result<(), NetNsError> {
        if self.namespaces.contains_key(name) {
            return Err(NetNsError::AlreadyExists(name.to_string()));
        }

        let ns_path = self.base_dir.join(name);
        
        // Check if namespace file already exists
        if ns_path.exists() {
            return Err(NetNsError::AlreadyExists(name.to_string()));
        }

        debug!("Creating namespace: {}", name);

        // Create an empty file for the namespace
        fs::File::create(&ns_path)
            .await
            .map_err(NetNsError::CreateFile)?;

        // Get current process network namespace
        let current_ns_path = format!("/proc/{}/ns/net", getpid());
        
        // Bind mount current netns to the new file
        mount(
            Some(current_ns_path.as_str()),
            &ns_path,
            None::<&str>,
            MsFlags::MS_BIND,
            None::<&str>,
        )
        .map_err(NetNsError::Mount)?;

        // Create a new network namespace by unsharing
        let clone_flags = CloneFlags::CLONE_NEWNET;
        
        // Fork and unshare in child, then bind mount the new namespace
        let result = tokio::task::spawn_blocking({
            let ns_path = ns_path.clone();
            let name = name.to_string();
            move || -> Result<(), NetNsError> {
                unsafe {
                    // Unshare network namespace
                    if nix::sched::unshare(clone_flags).is_err() {
                        return Err(NetNsError::Permission);
                    }
                }

                // Bind mount the new namespace
                let new_ns_path = format!("/proc/{}/ns/net", getpid());
                mount(
                    Some(new_ns_path.as_str()),
                    &ns_path,
                    None::<&str>,
                    MsFlags::MS_BIND,
                    None::<&str>,
                )
                .map_err(NetNsError::Mount)?;

                debug!("Successfully created namespace: {}", name);
                Ok(())
            }
        }).await.map_err(|e| NetNsError::CreateFile(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        result?;

        // Open the namespace file for later use
        let file = OpenOptions::new()
            .read(true)
            .open(&ns_path)
            .map_err(NetNsError::OpenNs)?;
            
        self.namespaces.insert(name.to_string(), file);
        info!("Created namespace: {}", name);
        
        Ok(())
    }

    /// Delete a network namespace
    pub async fn delete_namespace(&mut self, name: &str) -> Result<(), NetNsError> {
        let ns_path = self.base_dir.join(name);
        
        if !ns_path.exists() {
            return Err(NetNsError::NotFound(name.to_string()));
        }

        debug!("Deleting namespace: {}", name);

        // Remove from our tracking
        self.namespaces.remove(name);

        // Unmount the namespace
        if let Err(e) = umount(&ns_path) {
            warn!("Failed to unmount namespace {}: {}", name, e);
        }

        // Remove the file
        fs::remove_file(&ns_path)
            .await
            .map_err(NetNsError::CreateFile)?;

        info!("Deleted namespace: {}", name);
        Ok(())
    }

    /// Enter a network namespace for the current thread
    pub fn enter_namespace(&self, name: &str) -> Result<NamespaceGuard, NetNsError> {
        let file = self.namespaces.get(name)
            .ok_or_else(|| NetNsError::NotFound(name.to_string()))?;

        // Save current namespace
        let current_ns = OpenOptions::new()
            .read(true)
            .open("/proc/self/ns/net")
            .map_err(NetNsError::OpenNs)?;

        // Enter the target namespace
        setns(&file, CloneFlags::CLONE_NEWNET)
            .map_err(NetNsError::SetNs)?;

        debug!("Entered namespace: {}", name);

        Ok(NamespaceGuard {
            original_ns: current_ns,
            current_name: name.to_string(),
        })
    }

    /// Get the file descriptor for a namespace
    pub fn get_namespace_fd(&self, name: &str) -> Result<RawFd, NetNsError> {
        let file = self.namespaces.get(name)
            .ok_or_else(|| NetNsError::NotFound(name.to_string()))?;
        Ok(file.as_raw_fd())
    }

    /// Check if a namespace exists
    pub fn namespace_exists(&self, name: &str) -> bool {
        self.namespaces.contains_key(name)
    }

    /// List all managed namespaces
    pub fn list_namespaces(&self) -> Vec<String> {
        self.namespaces.keys().cloned().collect()
    }

    /// Execute a closure in a specific namespace
    pub fn exec_in_namespace<F, T>(&self, name: &str, f: F) -> Result<T, NetNsError>
    where
        F: FnOnce() -> T,
    {
        let _guard = self.enter_namespace(name)?;
        Ok(f())
    }

    /// Execute an async closure in a specific namespace
    pub async fn exec_in_namespace_async<F, Fut, T>(&self, name: &str, f: F) -> Result<T, NetNsError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        let _guard = self.enter_namespace(name)?;
        Ok(f().await)
    }
}

impl Drop for Manager {
    fn drop(&mut self) {
        // Clean up all namespaces on drop
        let names: Vec<String> = self.namespaces.keys().cloned().collect();
        for name in names {
            if let Err(e) = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(self.delete_namespace(&name))
            }) {
                warn!("Failed to clean up namespace {}: {}", name, e);
            }
        }
    }
}

/// RAII guard for namespace entry/exit
pub struct NamespaceGuard {
    original_ns: File,
    current_name: String,
}

impl Drop for NamespaceGuard {
    fn drop(&mut self) {
        // Restore original namespace
        if let Err(e) = setns(&self.original_ns, CloneFlags::CLONE_NEWNET) {
            warn!("Failed to restore original namespace from {}: {}", self.current_name, e);
        } else {
            debug!("Restored original namespace from {}", self.current_name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[cfg(feature = "sudo-tests")]
    async fn test_namespace_creation() -> Result<(), NetNsError> {
        let mut manager = Manager::new()?;
        
        // Create a test namespace
        manager.create_namespace("test-ns").await?;
        assert!(manager.namespace_exists("test-ns"));
        
        // Try to create duplicate - should fail
        assert!(manager.create_namespace("test-ns").await.is_err());
        
        // Delete the namespace
        manager.delete_namespace("test-ns").await?;
        assert!(!manager.namespace_exists("test-ns"));
        
        Ok(())
    }

    #[tokio::test]
    #[cfg(feature = "sudo-tests")]
    async fn test_namespace_entry() -> Result<(), NetNsError> {
        let mut manager = Manager::new()?;
        
        manager.create_namespace("test-entry").await?;
        
        // Execute something in the namespace
        let result = manager.exec_in_namespace("test-entry", || {
            // In here we're in the test namespace
            42
        })?;
        
        assert_eq!(result, 42);
        
        manager.delete_namespace("test-entry").await?;
        Ok(())
    }
}