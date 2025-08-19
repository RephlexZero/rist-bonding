//! Virtual Ethernet (veth) interface management
//!
//! This module provides functionality to create veth pairs, move interfaces
//! between namespaces, configure MTU, and bring interfaces up/down.

use crate::netns::{Manager as NetNsManager, NetNsError};
use rtnetlink::{Handle, new_connection};
use futures::TryStreamExt;
use std::collections::HashMap;
use thiserror::Error;
use tokio::time::{sleep, Duration};
use tracing::{debug, info, warn};

#[derive(Error, Debug)]
pub enum VethError {
    #[error("I/O error: {0}")]
    Io(std::io::Error),
    
    #[error("Netlink connection failed: {0}")]
    Connection(rtnetlink::Error),
    
    #[error("Interface '{0}' not found")]
    NotFound(String),
    
    #[error("Interface '{0}' already exists")]
    AlreadyExists(String),
    
    #[error("Failed to create veth pair: {0}")]
    CreateFailed(rtnetlink::Error),
    
    #[error("Failed to move interface to namespace: {0}")]
    MoveFailed(rtnetlink::Error),
    
    #[error("Failed to bring interface up: {0}")]
    SetUpFailed(rtnetlink::Error),
    
    #[error("Failed to set MTU: {0}")]
    SetMtuFailed(rtnetlink::Error),
    
    #[error("Namespace error: {0}")]
    NetNs(#[from] NetNsError),
    
    #[error("Invalid interface name: {0}")]
    InvalidName(String),
}

/// Information about a veth interface
#[derive(Clone, Debug)]
pub struct VethInfo {
    pub name: String,
    pub index: u32,
    pub peer_name: String,
    pub peer_index: Option<u32>,
    pub mtu: u32,
    pub namespace: Option<String>,
}

/// Veth pair manager
pub struct PairManager {
    /// Netlink handle for the default namespace
    handle: Handle,
    /// Track created veth pairs
    pairs: HashMap<String, VethPair>,
}

#[derive(Clone, Debug)]
pub struct VethPair {
    pub left: VethInfo,
    pub right: VethInfo,
}

impl PairManager {
    /// Create a new veth pair manager
    pub async fn new() -> Result<Self, VethError> {
        let (connection, handle, _) = new_connection()
            .map_err(VethError::Io)?;
            
        // Spawn the netlink connection
        tokio::spawn(connection);
        
        Ok(Self {
            handle,
            pairs: HashMap::new(),
        })
    }

    /// Create a veth pair with the given names
    pub async fn create_pair(&mut self, left_name: &str, right_name: &str) -> Result<VethPair, VethError> {
        if self.pairs.contains_key(left_name) || self.pairs.contains_key(right_name) {
            return Err(VethError::AlreadyExists(format!("{}/{}", left_name, right_name)));
        }

        // Validate interface names
        if !is_valid_interface_name(left_name) {
            return Err(VethError::InvalidName(left_name.to_string()));
        }
        if !is_valid_interface_name(right_name) {
            return Err(VethError::InvalidName(right_name.to_string()));
        }

        debug!("Creating veth pair: {} <-> {}", left_name, right_name);

        // Use rtnetlink's veth builder
        self.handle
            .link()
            .add()
            .veth(left_name.to_string(), right_name.to_string())
            .execute()
            .await
            .map_err(VethError::CreateFailed)?;

        // Wait a moment for the interfaces to be created
        sleep(Duration::from_millis(100)).await;

        // Get interface information
        let left_info = self.get_interface_info(left_name).await?;
        let right_info = self.get_interface_info(right_name).await?;

        let pair = VethPair {
            left: VethInfo {
                name: left_name.to_string(),
                index: left_info.index,
                peer_name: right_name.to_string(),
                peer_index: Some(right_info.index),
                mtu: left_info.mtu,
                namespace: None,
            },
            right: VethInfo {
                name: right_name.to_string(),
                index: right_info.index,
                peer_name: left_name.to_string(),
                peer_index: Some(left_info.index),
                mtu: right_info.mtu,
                namespace: None,
            },
        };

        self.pairs.insert(left_name.to_string(), pair.clone());
        self.pairs.insert(right_name.to_string(), pair.clone());

        info!("Created veth pair: {} <-> {}", left_name, right_name);
        Ok(pair)
    }

    /// Move an interface to a network namespace
    pub async fn move_to_namespace(&mut self, interface_name: &str, target_ns: &str, ns_manager: &NetNsManager) -> Result<(), VethError> {
        debug!("Moving interface {} to namespace {}", interface_name, target_ns);

        let interface_index = self.get_interface_info(interface_name).await?.index;
        let ns_fd = ns_manager.get_namespace_fd(target_ns)?;

        // Move interface to namespace
        self.handle
            .link()
            .set(interface_index)
            .setns_by_fd(ns_fd)
            .execute()
            .await
            .map_err(VethError::MoveFailed)?;

        // Update our tracking
        if let Some(pair) = self.pairs.get_mut(interface_name) {
            if pair.left.name == interface_name {
                pair.left.namespace = Some(target_ns.to_string());
            } else if pair.right.name == interface_name {
                pair.right.namespace = Some(target_ns.to_string());
            }
        }

        info!("Moved interface {} to namespace {}", interface_name, target_ns);
        Ok(())
    }

    /// Set MTU for an interface (in its current namespace)
    pub async fn set_mtu(&mut self, interface_name: &str, mtu: u32, ns_manager: Option<&NetNsManager>) -> Result<(), VethError> {
        debug!("Setting MTU {} for interface {}", mtu, interface_name);

        // Determine which handle to use based on interface location
        let handle = if let Some(pair) = self.pairs.get(interface_name) {
            let interface_info = if pair.left.name == interface_name {
                &pair.left
            } else {
                &pair.right
            };

            if let Some(ns) = &interface_info.namespace {
                if let Some(ns_mgr) = ns_manager {
                    // Create a handle in the target namespace
                    let handle = ns_mgr.exec_in_namespace(ns, || {
                        tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current().block_on(async {
                                let (connection, handle, _) = new_connection()
                                    .map_err(VethError::Io)?;
                                tokio::spawn(connection);
                                Ok::<Handle, VethError>(handle)
                            })
                        })
                    })?;
                    handle?
                } else {
                    return Err(VethError::NetNs(NetNsError::NotFound(ns.clone())));
                }
            } else {
                self.handle.clone()
            }
        } else {
            self.handle.clone()
        };

        let interface_index = self.get_interface_info_with_handle(&handle, interface_name).await?.index;

        // Set MTU
        handle
            .link()
            .set(interface_index)
            .mtu(mtu)
            .execute()
            .await
            .map_err(VethError::SetMtuFailed)?;

        // Update our tracking
        if let Some(pair) = self.pairs.get_mut(interface_name) {
            if pair.left.name == interface_name {
                pair.left.mtu = mtu;
            } else if pair.right.name == interface_name {
                pair.right.mtu = mtu;
            }
        }

        info!("Set MTU {} for interface {}", mtu, interface_name);
        Ok(())
    }

    /// Bring an interface up
    pub async fn set_up(&self, interface_name: &str, ns_manager: Option<&NetNsManager>) -> Result<(), VethError> {
        debug!("Bringing interface {} up", interface_name);

        // Determine which handle to use
        let handle = if let Some(pair) = self.pairs.get(interface_name) {
            let interface_info = if pair.left.name == interface_name {
                &pair.left
            } else {
                &pair.right
            };

            if let Some(ns) = &interface_info.namespace {
                if let Some(ns_mgr) = ns_manager {
                    let handle = ns_mgr.exec_in_namespace(ns, || {
                        tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current().block_on(async {
                                let (connection, handle, _) = new_connection()
                                    .map_err(VethError::Io)?;
                                tokio::spawn(connection);
                                Ok::<Handle, VethError>(handle)
                            })
                        })
                    })?;
                    handle?
                } else {
                    return Err(VethError::NetNs(NetNsError::NotFound(ns.clone())));
                }
            } else {
                self.handle.clone()
            }
        } else {
            self.handle.clone()
        };

        let interface_index = self.get_interface_info_with_handle(&handle, interface_name).await?.index;

        // Bring interface up
        handle
            .link()
            .set(interface_index)
            .up()
            .execute()
            .await
            .map_err(VethError::SetUpFailed)?;

        info!("Brought interface {} up", interface_name);
        Ok(())
    }

    /// Delete a veth pair
    pub async fn delete_pair(&mut self, interface_name: &str) -> Result<(), VethError> {
        let pair = self.pairs.remove(interface_name)
            .ok_or_else(|| VethError::NotFound(interface_name.to_string()))?;

        debug!("Deleting veth pair: {} <-> {}", pair.left.name, pair.right.name);

        // Remove the other end from tracking too
        let other_name = if pair.left.name == interface_name {
            &pair.right.name
        } else {
            &pair.left.name
        };
        self.pairs.remove(other_name);

        // Delete one end of the pair (this deletes both)
        let interface_index = self.get_interface_info(interface_name).await?.index;
        self.handle
            .link()
            .del(interface_index)
            .execute()
            .await
            .map_err(VethError::CreateFailed)?;

        info!("Deleted veth pair: {} <-> {}", pair.left.name, pair.right.name);
        Ok(())
    }

    /// Get information about an interface
    async fn get_interface_info(&self, name: &str) -> Result<InterfaceInfo, VethError> {
        self.get_interface_info_with_handle(&self.handle, name).await
    }

    /// Get information about an interface using a specific handle
    async fn get_interface_info_with_handle(&self, handle: &Handle, name: &str) -> Result<InterfaceInfo, VethError> {
        let mut links = handle
            .link()
            .get()
            .match_name(name.to_string())
            .execute();

        if let Some(link) = links.try_next().await.map_err(VethError::Connection)? {
            Ok(InterfaceInfo {
                index: link.header.index,
                name: name.to_string(),
                mtu: 1500, // Default MTU - we'll get actual value from rtnetlink API later
            })
        } else {
            Err(VethError::NotFound(name.to_string()))
        }
    }

    /// List all tracked veth pairs
    pub fn list_pairs(&self) -> Vec<&VethPair> {
        let mut pairs: Vec<&VethPair> = self.pairs.values().collect();
        pairs.sort_by(|a, b| a.left.name.cmp(&b.left.name));
        pairs.dedup_by(|a, b| a.left.name == b.left.name);
        pairs
    }
}

#[derive(Clone, Debug)]
struct InterfaceInfo {
    index: u32,
    name: String,
    mtu: u32,
}

/// Validate interface name according to Linux rules
fn is_valid_interface_name(name: &str) -> bool {
    !name.is_empty() 
        && name.len() <= 15 
        && name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
        && !name.starts_with('-')
}

impl Drop for PairManager {
    fn drop(&mut self) {
        // Clean up all veth pairs
        let pair_names: Vec<String> = self.pairs.keys().cloned().collect();
        for name in pair_names {
            if let Err(e) = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(self.delete_pair(&name))
            }) {
                warn!("Failed to clean up veth pair {}: {}", name, e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interface_name_validation() {
        assert!(is_valid_interface_name("eth0"));
        assert!(is_valid_interface_name("veth-test"));
        assert!(is_valid_interface_name("test_123"));
        
        assert!(!is_valid_interface_name(""));
        assert!(!is_valid_interface_name("this-name-is-way-too-long-for-linux"));
        assert!(!is_valid_interface_name("-invalid"));
        assert!(!is_valid_interface_name("invalid@name"));
    }

    #[tokio::test]
    #[cfg(feature = "sudo-tests")]
    async fn test_veth_creation() -> Result<(), VethError> {
        let mut manager = PairManager::new().await?;
        
        let pair = manager.create_pair("test-left", "test-right").await?;
        assert_eq!(pair.left.name, "test-left");
        assert_eq!(pair.right.name, "test-right");
        
        manager.delete_pair("test-left").await?;
        Ok(())
    }
}