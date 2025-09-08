//! Simple network simulation library
//!
//! This library provides utilities for applying fixed network parameters
//! to network interfaces using Linux network namespaces and qdisc.

pub mod namespace;
pub mod qdisc;
pub mod runtime;
pub mod types;

#[cfg(feature = "docker")]
pub mod docker;

pub use namespace::{
    cleanup_rist_test_links, cleanup_shaped_veth_pair, create_rist_test_links,
    create_shaped_veth_pair, exec_in_rx_namespace, get_connection_ips, ShapedVethConfig,
};
pub use runtime::{
    apply_ingress_params, apply_network_params, remove_ingress_params, remove_network_params,
};
pub use types::{NetworkParams, RuntimeError};
