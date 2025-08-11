//! Network emulation library for RIST testing
//!
//! This crate provides network namespace-based emulation with:
//! - Per-link network namespaces with veth pairs
//! - Ornstein-Uhlenbeck driven throughput variation
//! - Gilbert-Elliott burst loss modeling
//! - Netem delay/jitter/reorder effects
//! - All via netlink (no shell commands)

pub mod builder;
pub mod errors;
pub mod forwarder;
pub mod ge;
pub mod handle;
pub mod metrics;
pub mod ns;
pub mod ou;
pub mod qdisc;
pub mod types;
pub mod util;

// Re-exports for public API
pub use builder::{EmulatorBuilder, LinkBuilder};
pub use errors::NetemError;
pub use handle::{EmulatorHandle, LinkHandle};
pub use types::*;
