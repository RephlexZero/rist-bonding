//! HTTP server for metrics endpoints and observability dashboard

use crate::{Result, MetricsSnapshot};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Configuration for the observability server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityConfig {
    pub bind_address: String,
    pub port: u16,
    pub enable_cors: bool,
    pub dashboard_enabled: bool,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1".to_string(),
            port: 8080,
            enable_cors: true,
            dashboard_enabled: true,
        }
    }
}

/// HTTP server that provides metrics endpoints and optional dashboard
pub struct MetricsServer {
    config: ObservabilityConfig,
    latest_snapshot: Arc<RwLock<Option<MetricsSnapshot>>>,
}

impl MetricsServer {
    pub fn new(config: ObservabilityConfig) -> Self {
        Self {
            config,
            latest_snapshot: Arc::new(RwLock::new(None)),
        }
    }

    /// Start the HTTP server (stub implementation)
    pub async fn start(&self) -> Result<()> {
        tracing::info!(
            "Metrics server configured for http://{}:{}",
            self.config.bind_address,
            self.config.port
        );
        tracing::info!("Dashboard enabled: {}", self.config.dashboard_enabled);
        
        // TODO: Implement actual HTTP server
        // For now, just return success to allow the crate to compile
        Ok(())
    }

    /// Update the server state with a new metrics snapshot
    pub async fn update_snapshot(&self, snapshot: MetricsSnapshot) -> Result<()> {
        let mut state = self.latest_snapshot.write().await;
        *state = Some(snapshot);
        Ok(())
    }

    /// Get current snapshot for testing
    pub async fn get_snapshot(&self) -> Option<MetricsSnapshot> {
        self.latest_snapshot.read().await.clone()
    }
}