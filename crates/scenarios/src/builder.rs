//! Builder pattern for creating custom test scenarios
//!
//! This module provides ScenarioBuilder for constructing TestScenario 
//! instances with a fluent API.

use crate::link::LinkSpec;
use crate::scenario::TestScenario;
use std::collections::HashMap;
use std::time::Duration;

/// Scenario builder for creating custom scenarios
pub struct ScenarioBuilder {
    name: String,
    description: String,
    links: Vec<LinkSpec>,
    duration: Option<Duration>,
    metadata: HashMap<String, String>,
}

impl ScenarioBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            links: Vec::new(),
            duration: None,
            metadata: HashMap::new(),
        }
    }

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    pub fn add_link(mut self, link: LinkSpec) -> Self {
        self.links.push(link);
        self
    }

    pub fn duration(mut self, duration: Duration) -> Self {
        self.duration = Some(duration);
        self
    }

    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    pub fn build(self) -> TestScenario {
        TestScenario {
            name: self.name,
            description: self.description,
            links: self.links,
            duration_seconds: self.duration.map(|d| d.as_secs()),
            metadata: self.metadata,
        }
    }
}