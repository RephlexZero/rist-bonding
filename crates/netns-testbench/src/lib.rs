//! Network namespace testbench for RIST bonding
//!
//! This crate provides a Rust-only Linux network namespace testbench that can
//! create any number of links with independent, time-varying bandwidth, delay,
//! jitter, loss, reordering, MTU, and optional NAT configurations.
//!
//! The testbench uses Linux network namespaces with veth pairs and configures
//! qdiscs (netem + tbf/htb + fq_codel) via netlink, providing a drop-in
//! replacement for the current network-sim feature.

pub mod addr;
pub mod bench;
pub mod netns;
pub mod qdisc;
pub mod runtime;
pub mod veth;

// Re-export commonly used types
pub use bench::{LinkHandle, NetworkOrchestrator};
pub use scenarios::{DirectionSpec, LinkSpec, Schedule, TestScenario};

// Error types
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TestbenchError {
    #[error("Network namespace error: {0}")]
    NetNs(#[from] netns::NetNsError),

    #[error("Veth interface error: {0}")]
    Veth(#[from] veth::VethError),

    #[error("Address configuration error: {0}")]
    Addr(#[from] addr::AddrError),

    #[error("Qdisc configuration error: {0}")]
    Qdisc(#[from] qdisc::QdiscError),

    #[error("Runtime scheduler error: {0}")]
    Runtime(#[from] runtime::RuntimeError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Netlink error: {0}")]
    Netlink(#[from] rtnetlink::Error),

    #[error("System call error: {0}")]
    Nix(#[from] nix::Error),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}
