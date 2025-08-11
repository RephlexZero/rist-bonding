//! Builder pattern for creating network emulators

use crate::errors::{NetemError, Result};
use crate::handle::EmulatorHandle;
use crate::types::{LinkSpec, Scenario};
use crate::util::{validate_ge_params, validate_ou_params};

/// Builder for creating network emulators
pub struct EmulatorBuilder {
    links: Vec<LinkSpec>,
    seed: Option<u64>,
}

impl EmulatorBuilder {
    pub fn new() -> Self {
        Self {
            links: Vec::new(),
            seed: None,
        }
    }

    /// Add a link to the emulator
    pub fn add_link(&mut self, spec: LinkSpec) -> &mut Self {
        self.links.push(spec);
        self
    }

    /// Set random seed for reproducibility
    pub fn with_seed(&mut self, seed: u64) -> &mut Self {
        self.seed = Some(seed);
        self
    }

    /// Load configuration from JSON scenario
    pub fn from_json(json: &str) -> Result<Self> {
        let scenario: Scenario = serde_json::from_str(json)?;

        let mut builder = Self::new();

        for link in scenario.links {
            builder.add_link(link);
        }

        if let Some(seed) = scenario.seed {
            builder.with_seed(seed);
        }

        Ok(builder)
    }

    /// Load configuration from JSON file
    pub async fn from_file(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let json = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| NetemError::Io(e))?;
        Self::from_json(&json)
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        if self.links.is_empty() {
            return Err(NetemError::InvalidParameter(
                "No links specified".to_string(),
            ));
        }

        // Check for duplicate link names
        let mut names = std::collections::HashSet::new();
        for link in &self.links {
            if !names.insert(&link.name) {
                return Err(NetemError::InvalidParameter(format!(
                    "Duplicate link name: {}",
                    link.name
                )));
            }
        }

        // Validate each link's parameters
        for link in &self.links {
            validate_ou_params(&link.ou)?;
            validate_ge_params(&link.ge)?;
        }

        Ok(())
    }

    /// Build the emulator handle (creates namespaces, veths, qdiscs)
    pub async fn build(self) -> Result<EmulatorHandle> {
        self.validate()?;
        EmulatorHandle::new(self.links, self.seed).await
    }
}

impl Default for EmulatorBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for individual link specifications
pub struct LinkBuilder {
    spec: LinkSpec,
}

impl LinkBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            spec: LinkSpec::new(name),
        }
    }

    /// Set the rate limiter
    pub fn rate_limiter(mut self, limiter: crate::types::RateLimiter) -> Self {
        self.spec.rate_limiter = limiter;
        self
    }

    /// Set OU parameters
    pub fn ou_params(mut self, ou: crate::types::OUParams) -> Self {
        self.spec.ou = ou;
        self
    }

    /// Set GE parameters
    pub fn ge_params(mut self, ge: crate::types::GEParams) -> Self {
        self.spec.ge = ge;
        self
    }

    /// Set delay profile
    pub fn delay_profile(mut self, delay: crate::types::DelayProfile) -> Self {
        self.spec.delay = delay;
        self
    }

    /// Enable ingress shaping
    pub fn with_ingress_shaping(mut self, enabled: bool) -> Self {
        self.spec.ifb_ingress = enabled;
        self
    }

    /// Build the link specification
    pub fn build(self) -> LinkSpec {
        self.spec
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;

    #[test]
    fn test_builder_basic() {
        let mut builder = EmulatorBuilder::new();
        let link = LinkSpec::new("test-link");
        builder.add_link(link).with_seed(42);

        assert_eq!(builder.links.len(), 1);
        assert_eq!(builder.seed, Some(42));
    }

    #[test]
    fn test_link_builder() {
        let link = LinkBuilder::new("test")
            .ou_params(OUParams {
                mean_bps: 1_000_000,
                tau_ms: 1000,
                sigma: 0.2,
                tick_ms: 100,
            })
            .ge_params(GEParams {
                p_good: 0.001,
                p_bad: 0.1,
                p: 0.01,
                r: 0.1,
            })
            .delay_profile(DelayProfile {
                delay_ms: 20,
                jitter_ms: 5,
                reorder_pct: 0.0,
            })
            .with_ingress_shaping(true)
            .build();

        assert_eq!(link.name, "test");
        assert_eq!(link.ou.mean_bps, 1_000_000);
        assert!(link.ifb_ingress);
    }

    #[test]
    fn test_validation() {
        // Empty builder should fail validation
        let builder = EmulatorBuilder::new();
        assert!(builder.validate().is_err());

        // Duplicate names should fail
        let mut builder = EmulatorBuilder::new();
        builder.add_link(LinkSpec::new("test"));
        builder.add_link(LinkSpec::new("test"));
        assert!(builder.validate().is_err());

        // Valid builder should pass
        let mut builder = EmulatorBuilder::new();
        builder.add_link(LinkSpec::new("link1"));
        builder.add_link(LinkSpec::new("link2"));
        assert!(builder.validate().is_ok());
    }

    #[test]
    fn test_json_scenario() {
        let json = r#"
        {
            "links": [
                {
                    "name": "cell0",
                    "rate_limiter": "Tbf",
                    "ou": {
                        "mean_bps": 1000000,
                        "tau_ms": 1000,
                        "sigma": 0.2,
                        "tick_ms": 100
                    },
                    "ge": {
                        "p_good": 0.001,
                        "p_bad": 0.1,
                        "p": 0.01,
                        "r": 0.1
                    },
                    "delay": {
                        "delay_ms": 20,
                        "jitter_ms": 5,
                        "reorder_pct": 0.0
                    },
                    "ifb_ingress": false
                }
            ],
            "seed": 42
        }
        "#;

        let builder = EmulatorBuilder::from_json(json).expect("Should parse JSON");
        assert_eq!(builder.links.len(), 1);
        assert_eq!(builder.seed, Some(42));
        assert_eq!(builder.links[0].name, "cell0");
    }
}
