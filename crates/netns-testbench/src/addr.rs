//! IP address and routing configuration
//!
//! This module provides functionality to configure IP addresses, routes,
//! and bring up loopback interfaces in network namespaces.

use crate::netns::{Manager as NetNsManager, NetNsError};
use futures::TryStreamExt;
use ipnetwork::{IpNetwork, Ipv4Network};
use rtnetlink::{new_connection, Handle};
use std::net::{IpAddr, Ipv4Addr};
use thiserror::Error;
use tracing::{debug, error, info};

#[derive(Error, Debug)]
pub enum AddrError {
    #[error("I/O error: {0}")]
    Io(std::io::Error),

    #[error("Netlink connection failed: {0}")]
    Connection(rtnetlink::Error),

    #[error("Failed to add address: {0}")]
    AddAddress(rtnetlink::Error),

    #[error("Failed to add route: {0}")]
    AddRoute(rtnetlink::Error),

    #[error("Failed to configure loopback: {0}")]
    Loopback(rtnetlink::Error),

    #[error("Interface not found: {0}")]
    InterfaceNotFound(String),

    #[error("Namespace error: {0}")]
    NetNs(#[from] NetNsError),

    #[error("Invalid network configuration: {0}")]
    InvalidConfig(String),
}

/// Address configuration manager
pub struct Configurer {
    /// Default netlink handle (for host namespace)
    handle: Handle,
}

#[derive(Clone, Debug)]
pub struct AddressConfig {
    pub interface: String,
    pub address: IpNetwork,
    pub namespace: Option<String>,
}

#[derive(Clone, Debug)]
pub struct RouteConfig {
    pub destination: IpNetwork,
    pub gateway: Option<IpAddr>,
    pub interface: Option<String>,
    pub namespace: Option<String>,
}

impl Configurer {
    /// Create a new address configurer
    pub async fn new() -> Result<Self, AddrError> {
        let (connection, handle, _) = new_connection().map_err(AddrError::Io)?;

        tokio::spawn(connection);

        Ok(Self { handle })
    }

    /// Add an IP address to an interface
    pub async fn add_address(
        &self,
        config: AddressConfig,
        ns_manager: Option<&NetNsManager>,
    ) -> Result<(), AddrError> {
        debug!(
            "Adding address {} to interface {} in namespace {:?}",
            config.address, config.interface, config.namespace
        );

        let handle = if let Some(ns) = &config.namespace {
            if let Some(ns_mgr) = ns_manager {
                self.create_ns_handle(ns_mgr, ns).await?
            } else {
                return Err(AddrError::NetNs(NetNsError::NotFound(ns.clone())));
            }
        } else {
            self.handle.clone()
        };

        // Find the interface
        let interface_index = self
            .find_interface_index(&handle, &config.interface)
            .await?;

        // Add the address - handle "already exists" gracefully
        let prefix_len = config.address.prefix();
        let ip = config.address.ip();
        match handle
            .address()
            .add(interface_index, ip, prefix_len)
            .execute()
            .await
        {
            Ok(_) => {
                info!(
                    "Added address {} to interface {} in namespace {:?}",
                    config.address, config.interface, config.namespace
                );
                Ok(())
            }
            Err(e) => {
                // If address already exists, that's okay - continue
                let error_msg = e.to_string();
                if error_msg.contains("File exists")
                    || error_msg.contains("EEXIST")
                    || error_msg.contains("os error 17")
                {
                    debug!(
                        "Address {} already exists on interface {} ({}), continuing",
                        config.address, config.interface, error_msg
                    );
                    Ok(())
                } else {
                    error!("Failed to add address {}: {}", config.address, error_msg);
                    Err(AddrError::AddAddress(e))
                }
            }
        }
    }

    /// Add a route
    pub async fn add_route(
        &self,
        config: RouteConfig,
        ns_manager: Option<&NetNsManager>,
    ) -> Result<(), AddrError> {
        debug!(
            "Adding route {} via {:?} dev {:?} in namespace {:?}",
            config.destination, config.gateway, config.interface, config.namespace
        );

        let handle = if let Some(ns) = &config.namespace {
            if let Some(ns_mgr) = ns_manager {
                self.create_ns_handle(ns_mgr, ns).await?
            } else {
                return Err(AddrError::NetNs(NetNsError::NotFound(ns.clone())));
            }
        } else {
            self.handle.clone()
        };

        // Separate v4/v6 route addition to avoid type mismatch
        match config.destination {
            IpNetwork::V4(net) => {
                let mut route_builder = handle
                    .route()
                    .add()
                    .v4()
                    .destination_prefix(net.ip(), net.prefix());
                if let Some(gw) = config.gateway {
                    if let IpAddr::V4(gw4) = gw {
                        route_builder = route_builder.gateway(gw4);
                    }
                }
                if let Some(iface) = &config.interface {
                    let interface_index = self.find_interface_index(&handle, iface).await?;
                    route_builder = route_builder.output_interface(interface_index);
                }
                route_builder.execute().await.map_err(AddrError::AddRoute)?;
            }
            IpNetwork::V6(net) => {
                let mut route_builder = handle
                    .route()
                    .add()
                    .v6()
                    .destination_prefix(net.ip(), net.prefix());
                if let Some(gw) = config.gateway {
                    if let IpAddr::V6(gw6) = gw {
                        route_builder = route_builder.gateway(gw6);
                    }
                }
                if let Some(iface) = &config.interface {
                    let interface_index = self.find_interface_index(&handle, iface).await?;
                    route_builder = route_builder.output_interface(interface_index);
                }
                route_builder.execute().await.map_err(AddrError::AddRoute)?;
            }
        }

        info!(
            "Added route {} via {:?} dev {:?} in namespace {:?}",
            config.destination, config.gateway, config.interface, config.namespace
        );
        Ok(())
    }

    /// Configure loopback interface in a namespace
    pub async fn configure_loopback(
        &self,
        namespace: &str,
        ns_manager: &NetNsManager,
    ) -> Result<(), AddrError> {
        debug!("Configuring loopback in namespace {}", namespace);

        let handle = self.create_ns_handle(ns_manager, namespace).await?;

        // Find loopback interface
        let lo_index = self.find_interface_index(&handle, "lo").await?;

        // Add loopback address
        match handle
            .address()
            .add(lo_index, IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8)
            .execute()
            .await
        {
            Ok(_) => {}
            Err(e) => {
                let msg = e.to_string();
                // If already configured, that's fine
                if msg.contains("File exists")
                    || msg.contains("EEXIST")
                    || msg.contains("os error 17")
                {
                    debug!(
                        "Loopback address already present in {}, continuing",
                        namespace
                    );
                } else {
                    return Err(AddrError::AddAddress(e));
                }
            }
        }

        // Bring loopback up
        if let Err(e) = handle.link().set(lo_index).up().execute().await {
            let msg = e.to_string();
            // Some kernels might return harmless errors if already up; log and continue
            if msg.contains("File exists")
                || msg.contains("EEXIST")
                || msg.contains("Device or resource busy")
            {
                debug!("Loopback already up in {} ({}), continuing", namespace, msg);
            } else {
                return Err(AddrError::Loopback(e));
            }
        }

        info!("Configured loopback in namespace {}", namespace);
        Ok(())
    }

    /// Set up a point-to-point link between two namespaces
    pub async fn setup_p2p_link(
        &self,
        left_ns: &str,
        left_iface: &str,
        left_addr: Ipv4Network,
        right_ns: &str,
        right_iface: &str,
        right_addr: Ipv4Network,
        ns_manager: &NetNsManager,
    ) -> Result<(), AddrError> {
        debug!(
            "Setting up P2P link: {}@{} ({}) <-> {}@{} ({})",
            left_iface, left_ns, left_addr, right_iface, right_ns, right_addr
        );

        // Configure left side
        self.add_address(
            AddressConfig {
                interface: left_iface.to_string(),
                address: IpNetwork::V4(left_addr),
                namespace: Some(left_ns.to_string()),
            },
            Some(ns_manager),
        )
        .await?;

        // Configure right side
        self.add_address(
            AddressConfig {
                interface: right_iface.to_string(),
                address: IpNetwork::V4(right_addr),
                namespace: Some(right_ns.to_string()),
            },
            Some(ns_manager),
        )
        .await?;

        // Add routes to each other
        self.add_route(
            RouteConfig {
                destination: IpNetwork::V4(right_addr),
                gateway: None,
                interface: Some(left_iface.to_string()),
                namespace: Some(left_ns.to_string()),
            },
            Some(ns_manager),
        )
        .await?;

        self.add_route(
            RouteConfig {
                destination: IpNetwork::V4(left_addr),
                gateway: None,
                interface: Some(right_iface.to_string()),
                namespace: Some(right_ns.to_string()),
            },
            Some(ns_manager),
        )
        .await?;

        info!(
            "Set up P2P link: {}@{} ({}) <-> {}@{} ({})",
            left_iface, left_ns, left_addr, right_iface, right_ns, right_addr
        );
        Ok(())
    }

    /// Generate a /30 subnet for point-to-point links
    pub fn generate_p2p_subnet(link_id: u8) -> Result<(Ipv4Network, Ipv4Network), AddrError> {
        if link_id == 0 {
            return Err(AddrError::InvalidConfig("Link ID cannot be 0".to_string()));
        }

        // For /30 networks, valid subnets start at multiples of 4.
        // Pick a base that is aligned on 4: 8, 12, 16, ...
        // Then use base+1 and base+2 as the two host addresses.
        // Example for link_id=1: base=8 -> hosts 10.0.0.9 and 10.0.0.10
        let base = 8 + (link_id as u32 - 1) * 4; // 10.0.0.8, 10.0.0.12, ...

        if base > 252 {
            return Err(AddrError::InvalidConfig(format!(
                "Link ID {} too high",
                link_id
            )));
        }

        let left_addr = Ipv4Network::new(Ipv4Addr::new(10, 0, 0, base as u8 + 1), 30)
            .map_err(|e| AddrError::InvalidConfig(e.to_string()))?;
        let right_addr = Ipv4Network::new(Ipv4Addr::new(10, 0, 0, base as u8 + 2), 30)
            .map_err(|e| AddrError::InvalidConfig(e.to_string()))?;

        Ok((left_addr, right_addr))
    }

    /// Create a netlink handle in a specific namespace
    async fn create_ns_handle(
        &self,
        ns_manager: &NetNsManager,
        namespace: &str,
    ) -> Result<Handle, AddrError> {
        let handle = ns_manager.exec_in_namespace(namespace, || {
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    let (connection, handle, _) = new_connection().map_err(AddrError::Io)?;
                    tokio::spawn(connection);
                    Ok::<Handle, AddrError>(handle)
                })
            })
        })?;
        handle
    }

    /// Find interface index by name
    async fn find_interface_index(&self, handle: &Handle, name: &str) -> Result<u32, AddrError> {
        let mut links = handle.link().get().match_name(name.to_string()).execute();

        if let Some(link) = links.try_next().await.map_err(AddrError::Connection)? {
            Ok(link.header.index)
        } else {
            Err(AddrError::InterfaceNotFound(name.to_string()))
        }
    }
}

/// Builder for common network configurations
pub struct NetworkConfigBuilder {
    addresses: Vec<AddressConfig>,
    routes: Vec<RouteConfig>,
}

impl NetworkConfigBuilder {
    pub fn new() -> Self {
        Self {
            addresses: Vec::new(),
            routes: Vec::new(),
        }
    }

    pub fn add_address(mut self, config: AddressConfig) -> Self {
        self.addresses.push(config);
        self
    }

    pub fn add_route(mut self, config: RouteConfig) -> Self {
        self.routes.push(config);
        self
    }

    /// Add a simple P2P configuration
    pub fn p2p_link(
        mut self,
        left_ns: String,
        left_iface: String,
        left_addr: Ipv4Network,
        right_ns: String,
        right_iface: String,
        right_addr: Ipv4Network,
    ) -> Self {
        self.addresses.push(AddressConfig {
            interface: left_iface.clone(),
            address: IpNetwork::V4(left_addr),
            namespace: Some(left_ns.clone()),
        });

        self.addresses.push(AddressConfig {
            interface: right_iface.clone(),
            address: IpNetwork::V4(right_addr),
            namespace: Some(right_ns.clone()),
        });

        self.routes.push(RouteConfig {
            destination: IpNetwork::V4(right_addr),
            gateway: None,
            interface: Some(left_iface),
            namespace: Some(left_ns),
        });

        self.routes.push(RouteConfig {
            destination: IpNetwork::V4(left_addr),
            gateway: None,
            interface: Some(right_iface),
            namespace: Some(right_ns),
        });

        self
    }

    pub async fn apply(
        self,
        configurer: &Configurer,
        ns_manager: &NetNsManager,
    ) -> Result<(), AddrError> {
        // Apply addresses first
        for addr_config in self.addresses {
            configurer
                .add_address(addr_config, Some(ns_manager))
                .await?;
        }

        // Then apply routes
        for route_config in self.routes {
            configurer.add_route(route_config, Some(ns_manager)).await?;
        }

        Ok(())
    }
}

impl Default for NetworkConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_p2p_subnet_generation() {
        let (left, right) = Configurer::generate_p2p_subnet(1).unwrap();
        // /30 aligned network starting at .8 -> usable hosts .9 and .10
        assert_eq!(left.ip(), Ipv4Addr::new(10, 0, 0, 9));
        assert_eq!(right.ip(), Ipv4Addr::new(10, 0, 0, 10));
        assert_eq!(left.prefix(), 30);
        assert_eq!(right.prefix(), 30);

        let (left2, right2) = Configurer::generate_p2p_subnet(2).unwrap();
        // Next /30 aligned network starting at .12 -> usable hosts .13 and .14
        assert_eq!(left2.ip(), Ipv4Addr::new(10, 0, 0, 13));
        assert_eq!(right2.ip(), Ipv4Addr::new(10, 0, 0, 14));
    }

    #[test]
    fn test_invalid_subnet_generation() {
        assert!(Configurer::generate_p2p_subnet(0).is_err());
        assert!(Configurer::generate_p2p_subnet(64).is_err()); // Would exceed 255
    }
}
