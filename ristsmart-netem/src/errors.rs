//! Error types for the network emulator

use thiserror::Error;

#[derive(Error, Debug)]
pub enum NetemError {
    #[error("Failed to create network namespace: {0}")]
    CreateNamespace(String),

    #[error("Failed to open network namespace: {0}")]
    NetNsOpen(String),

    #[error("Failed to cleanup network namespace: {0}")]
    NetNsCleanup(String),

    #[error("Failed to create veth pair: {0}")]
    CreateVeth(String),

    #[error("Failed to configure veth: {0}")]
    VethConfig(String),

    #[error("Failed to configure interface: {0}")]
    ConfigureInterface(String),

    #[error("Failed to apply qdisc: {0}")]
    QdiscApply(String),

    #[error("Failed to bind forwarder: {0}")]
    ForwarderBind(String),

    #[error("Failed to set network namespace: {0}")]
    SetNetNs(String),

    #[error("Component not initialized: {0}")]
    NotInitialized(String),

    #[error("Namespace not found: {0}")]
    NamespaceNotFound(String),

    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),

    #[error("Netlink error: {0}")]
    Netlink(#[from] rtnetlink::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Nix error: {0}")]
    Nix(#[from] nix::Error),

    #[error("Link not found: {0}")]
    LinkNotFound(String),
}

pub type Result<T> = std::result::Result<T, NetemError>;
