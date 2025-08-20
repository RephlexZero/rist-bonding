//! Test scenario definitions with preset implementations
//!
//! This module provides TestScenario for combining multiple links
//! into complete test scenarios with metadata and timing information.

use crate::direction::DirectionSpec;
use crate::link::LinkSpec;
use crate::schedule::Schedule;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// Test scenario combining multiple links and metadata
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestScenario {
    pub name: String,
    pub description: String,
    pub links: Vec<LinkSpec>,
    pub duration_seconds: Option<u64>,
    pub metadata: HashMap<String, String>,
}

impl TestScenario {
    /// Single good quality link
    pub fn baseline_good() -> Self {
        Self {
            name: "baseline_good".to_string(),
            description: "Single high-quality link for baseline testing".to_string(),
            links: vec![LinkSpec::symmetric(
                "primary".to_string(),
                "tx0".to_string(),
                "rx0".to_string(),
                Schedule::Constant(DirectionSpec::good()),
            )],
            duration_seconds: Some(30),
            metadata: HashMap::new(),
        }
    }

    /// Dual-link bonding with asymmetric quality
    pub fn bonding_asymmetric() -> Self {
        let mut metadata = HashMap::new();
        metadata.insert("test_type".to_string(), "bonding".to_string());
        metadata.insert(
            "expected_behavior".to_string(),
            "weight_adaptation".to_string(),
        );

        Self {
            name: "bonding_asymmetric".to_string(),
            description: "Two links with different quality for bonding tests".to_string(),
            links: vec![
                LinkSpec::symmetric(
                    "primary".to_string(),
                    "tx0".to_string(),
                    "rx0".to_string(),
                    Schedule::Constant(DirectionSpec::typical()),
                ),
                LinkSpec::symmetric(
                    "secondary".to_string(),
                    "tx1".to_string(),
                    "rx1".to_string(),
                    Schedule::Constant(DirectionSpec::poor()),
                ),
            ],
            duration_seconds: Some(120),
            metadata,
        }
    }

    /// Mobile network with handover events
    pub fn mobile_handover() -> Self {
        Self {
            name: "mobile_handover".to_string(),
            description: "Mobile network with simulated handover events".to_string(),
            links: vec![LinkSpec::asymmetric_cellular(
                "cellular".to_string(),
                "tx0".to_string(),
                "rx0".to_string(),
            )],
            duration_seconds: Some(180),
            metadata: HashMap::new(),
        }
    }

    /// Degrading network scenario
    pub fn degrading_network() -> Self {
        let good = DirectionSpec::good();
        let poor = DirectionSpec::poor();

        Self {
            name: "degrading_network".to_string(),
            description: "Network that starts good and degrades over time".to_string(),
            links: vec![LinkSpec::symmetric(
                "degrading".to_string(),
                "tx0".to_string(),
                "rx0".to_string(),
                Schedule::degradation_cycle(good, poor),
            )],
            duration_seconds: Some(120),
            metadata: HashMap::new(),
        }
    }

    /// 5G to LTE handover scenario
    pub fn nr_to_lte_handover() -> Self {
        let nr_good = DirectionSpec::nr_good();
        let lte_edge = DirectionSpec::lte_downlink();

        Self {
            name: "nr_to_lte_handover".to_string(),
            description: "5G to LTE handover with quality degradation".to_string(),
            links: vec![LinkSpec::symmetric(
                "handover".to_string(),
                "tx0".to_string(),
                "rx0".to_string(),
                Schedule::Steps(vec![
                    (Duration::from_secs(0), nr_good),
                    (
                        Duration::from_secs(60),
                        lte_edge.clone().with_handover_spike(),
                    ),
                    (Duration::from_secs(65), lte_edge),
                ]),
            )],
            duration_seconds: Some(120),
            metadata: HashMap::new(),
        }
    }

    /// mmWave with beam blockage scenario
    pub fn nr_mmwave_mobility() -> Self {
        Self {
            name: "nr_mmwave_mobility".to_string(),
            description: "5G mmWave with beam blockage events during mobility".to_string(),
            links: vec![LinkSpec::symmetric(
                "mmwave".to_string(),
                "tx0".to_string(),
                "rx0".to_string(),
                Schedule::Steps(vec![
                    (Duration::from_secs(0), DirectionSpec::nr_mmwave()),
                    (
                        Duration::from_secs(30),
                        DirectionSpec::nr_mmwave().with_mmwave_blockage(1.0),
                    ),
                    (Duration::from_secs(33), DirectionSpec::nr_mmwave()), // Quick recovery
                    (
                        Duration::from_secs(60),
                        DirectionSpec::nr_mmwave().with_mmwave_blockage(0.5),
                    ),
                    (Duration::from_secs(65), DirectionSpec::nr_mmwave()),
                ]),
            )],
            duration_seconds: Some(120),
            metadata: HashMap::new(),
        }
    }

    /// 5G network slicing scenario with different service types
    pub fn nr_network_slicing() -> Self {
        let mut metadata = HashMap::new();
        metadata.insert("test_type".to_string(), "network_slicing".to_string());
        metadata.insert("slices".to_string(), "urllc,embb,mmtc".to_string());

        Self {
            name: "nr_network_slicing".to_string(),
            description: "Multi-link 5G with different network slicing characteristics".to_string(),
            links: vec![
                LinkSpec::symmetric(
                    "urllc".to_string(),
                    "tx0".to_string(),
                    "rx0".to_string(),
                    Schedule::Constant(DirectionSpec::nr_urllc()),
                ),
                LinkSpec::symmetric(
                    "embb".to_string(),
                    "tx1".to_string(),
                    "rx1".to_string(),
                    Schedule::Constant(DirectionSpec::nr_embb()),
                ),
                LinkSpec::symmetric(
                    "mmtc".to_string(),
                    "tx2".to_string(),
                    "rx2".to_string(),
                    Schedule::Constant(DirectionSpec::nr_mmtc()),
                ),
            ],
            duration_seconds: Some(300),
            metadata,
        }
    }

    /// Carrier aggregation scenario
    pub fn nr_carrier_aggregation_test() -> Self {
        Self {
            name: "nr_carrier_aggregation_test".to_string(),
            description: "5G with carrier aggregation across multiple bands".to_string(),
            links: vec![LinkSpec::symmetric(
                "ca_link".to_string(),
                "tx0".to_string(),
                "rx0".to_string(),
                Schedule::Steps(vec![
                    (Duration::from_secs(0), DirectionSpec::nr_sub6ghz()),
                    (
                        Duration::from_secs(30),
                        DirectionSpec::nr_sub6ghz().with_carrier_aggregation(2),
                    ),
                    (
                        Duration::from_secs(60),
                        DirectionSpec::nr_sub6ghz().with_carrier_aggregation(3),
                    ),
                    (
                        Duration::from_secs(90),
                        DirectionSpec::nr_carrier_aggregation(),
                    ),
                ]),
            )],
            duration_seconds: Some(120),
            metadata: HashMap::new(),
        }
    }

    /// Beamforming interference scenario
    pub fn nr_beamforming_interference() -> Self {
        Self {
            name: "nr_beamforming_interference".to_string(),
            description: "5G beamforming with interference and beam steering effects".to_string(),
            links: vec![LinkSpec::symmetric(
                "beamform".to_string(),
                "tx0".to_string(),
                "rx0".to_string(),
                Schedule::bursty_markov(
                    DirectionSpec::nr_sub6ghz(),
                    DirectionSpec::nr_beamforming_interference(),
                ),
            )],
            duration_seconds: Some(180),
            metadata: HashMap::new(),
        }
    }
}