//! Simple network simulation library
//!
//! This library provides utilities for applying fixed network parameters
//! to network interfaces using Linux network namespaces and qdisc.

pub mod qdisc;
pub mod runtime;
pub mod types;

#[cfg(feature = "docker")]
pub mod docker;

pub use runtime::apply_network_params;
pub use types::{NetworkParams, RuntimeError};
