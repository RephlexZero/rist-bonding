//! Network namespace management
//!
//! This module provides functionality to create, delete, and enter Linux
//! network namespaces using the `/var/run/netns/<name>` convention.

use nix::mount::{mount, umount2, MntFlags, MsFlags};
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

    /// Attach to an existing namespace by name (opens the file and tracks it)
    pub fn attach_existing_namespace(&mut self, name: &str) -> Result<(), NetNsError> {
        let ns_path = self.base_dir.join(name);
        if !ns_path.exists() {
            return Err(NetNsError::NotFound(name.to_string()));
        }

        // Open the namespace file for later use
        let file = OpenOptions::new()
            .read(true)
            .open(&ns_path)
            .map_err(NetNsError::OpenNs)?;

        self.namespaces.insert(name.to_string(), file);
        Ok(())
    }

    /// Force cleanup of all stale namespaces with given prefix
    pub async fn force_cleanup_stale_namespaces(&mut self, prefix: &str) -> Result<usize, NetNsError> {
        let mut cleaned_count = 0;
        
        if let Ok(mut entries) = tokio::fs::read_dir(&self.base_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                if let Ok(name) = entry.file_name().into_string() {
                    if name.starts_with(prefix) {
                        debug!("Force cleaning stale namespace: {}", name);
                        if let Ok(()) = self.force_delete_namespace(&name).await {
                            cleaned_count += 1;
                        }
                    }
                }
            }
        }
        
        Ok(cleaned_count)
    }
    
    /// Force delete a namespace even if not in our tracking
    pub async fn force_delete_namespace(&mut self, name: &str) -> Result<(), NetNsError> {
        let ns_path = self.base_dir.join(name);
        
        debug!("Force deleting namespace: {}", name);
        
        // Remove from our tracking if present
        self.namespaces.remove(name);
        
    // Try to unmount first (prefer MNT_DETACH to avoid EBUSY)
        if ns_path.exists() {
            // Multiple unmount attempts in case of busy resources
            for attempt in 1..=3 {
        match umount2(&ns_path, MntFlags::MNT_DETACH) {
                    Ok(()) => break,
                    Err(e) if attempt < 3 => {
                        debug!("Unmount attempt {} failed for {}: {}, retrying", attempt, name, e);
                        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    },
                    Err(e) => warn!("Failed to unmount namespace {} after {} attempts: {}", name, attempt, e),
                }
            }
            
            // Force remove the file
            match tokio::fs::remove_file(&ns_path).await {
                Ok(()) => info!("Force deleted namespace: {}", name),
                Err(e) => {
                    // Try using `ip netns del` and then system rm as last resorts
                    let _ = tokio::process::Command::new("ip")
                        .args(["netns", "del", name])
                        .status()
                        .await;
                    let status = tokio::process::Command::new("rm")
                        .arg("-f")
                        .arg(&ns_path)
                        .status()
                        .await;
                    if let Ok(status) = status {
                        if status.success() {
                            info!("Force deleted namespace with rm: {}", name);
                        } else {
                            return Err(NetNsError::CreateFile(e));
                        }
                    } else {
                        return Err(NetNsError::CreateFile(e));
                    }
                },
            }
        }
        
        Ok(())
    }

    /// Create a new network namespace
    pub async fn create_namespace(&mut self, name: &str) -> Result<(), NetNsError> {
        if self.namespaces.contains_key(name) {
            return Err(NetNsError::AlreadyExists(name.to_string()));
        }

        let ns_path = self.base_dir.join(name);
        
        // Check if namespace file already exists and clean it up
        if ns_path.exists() {
            warn!("Cleaning up stale namespace file: {}", name);
            if let Err(e) = self.force_delete_namespace(name).await {
                warn!("Failed to clean up stale namespace {}: {}", name, e);
                return Err(NetNsError::AlreadyExists(name.to_string()));
            }
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
                // Unshare network namespace
                if nix::sched::unshare(clone_flags).is_err() {
                    return Err(NetNsError::Permission);
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
            // Remove from tracking even if file doesn't exist
            self.namespaces.remove(name);
            return Ok(());
        }

        debug!("Deleting namespace: {}", name);

        // Remove from our tracking first
        self.namespaces.remove(name);

        // Try graceful cleanup first, then force cleanup if needed
        match self.try_graceful_delete(&ns_path).await {
            Ok(()) => {
                info!("Deleted namespace: {}", name);
                Ok(())
            },
            Err(_) => {
                warn!("Graceful delete failed for {}, attempting force delete", name);
                self.force_delete_namespace(name).await
            }
        }
    }
    
    /// Try graceful namespace deletion
    async fn try_graceful_delete(&self, ns_path: &std::path::Path) -> Result<(), NetNsError> {
        // Unmount the namespace (lazy unmount to avoid EBUSY)
        umount2(ns_path, MntFlags::MNT_DETACH).map_err(NetNsError::Mount)?;

        // Remove the file
        fs::remove_file(ns_path)
            .await
            .map_err(NetNsError::CreateFile)?;
            
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
        // Best-effort synchronous cleanup without requiring a Tokio runtime
        let names: Vec<String> = self.namespaces.keys().cloned().collect();
        for name in names {
            let ns_path = self.base_dir.join(&name);
            // Try lazy unmount; ignore errors
            let _ = umount2(&ns_path, MntFlags::MNT_DETACH);
            // Try std::fs remove; if it fails, try shell fallbacks
            if let Err(e) = std::fs::remove_file(&ns_path) {
                // Try `ip netns del <name>` then rm -f
                let _ = std::process::Command::new("ip").args(["netns", "del", &name]).status();
                let _ = std::process::Command::new("rm").args(["-f", ns_path.to_string_lossy().as_ref()]).status();
                // As last resort, just warn
                let _ = e; // suppress unused if success
            }
        }
        self.namespaces.clear();
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
    // use super::{Manager, NetNsError};

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