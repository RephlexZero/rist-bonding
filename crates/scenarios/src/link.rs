//! Network link specifications with bidirectional characteristics
//!
//! This module provides LinkSpec for defining complete network links
//! with separate schedules for each direction (A->B and B->A).

use crate::direction::DirectionSpec;
use crate::schedule::Schedule;
use serde::{Deserialize, Serialize};

/// Complete link specification with bidirectional characteristics
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LinkSpec {
    /// Human-readable name for the link
    pub name: String,
    /// Left namespace name (e.g., "tx0")
    pub a_ns: String,
    /// Right namespace name (e.g., "rx0")
    pub b_ns: String,
    /// A->B direction schedule
    pub a_to_b: Schedule,
    /// B->A direction schedule
    pub b_to_a: Schedule,
}

impl LinkSpec {
    /// Create a symmetric link with the same schedule in both directions
    pub fn symmetric(name: String, a_ns: String, b_ns: String, schedule: Schedule) -> Self {
        Self {
            name,
            a_ns,
            b_ns,
            a_to_b: schedule.clone(),
            b_to_a: schedule,
        }
    }

    /// Create an asymmetric link (typical for cellular)
    pub fn asymmetric_cellular(name: String, a_ns: String, b_ns: String) -> Self {
        Self {
            name,
            a_ns,
            b_ns,
            a_to_b: Schedule::Constant(DirectionSpec::lte_downlink()),
            b_to_a: Schedule::Constant(DirectionSpec::lte_uplink()),
        }
    }
}
