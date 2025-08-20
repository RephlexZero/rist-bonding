//! Preset collections for common testing scenarios
//!
//! This module provides organized collections of pre-built test scenarios
//! for different network types and test cases.

use crate::scenario::TestScenario;

/// Preset collections for common testing scenarios
pub struct Presets;

impl Presets {
    /// Get all basic scenarios for testing
    pub fn basic_scenarios() -> Vec<TestScenario> {
        vec![
            TestScenario::baseline_good(),
            TestScenario::bonding_asymmetric(),
            TestScenario::degrading_network(),
        ]
    }

    /// Get mobile/cellular scenarios
    pub fn cellular_scenarios() -> Vec<TestScenario> {
        vec![
            TestScenario::mobile_handover(),
            TestScenario::nr_to_lte_handover(),
        ]
    }

    /// Get complex multi-link scenarios  
    pub fn multi_link_scenarios() -> Vec<TestScenario> {
        vec![
            TestScenario::nr_network_slicing(),
            TestScenario::nr_carrier_aggregation_test(),
        ]
    }

    /// Get 5G-specific scenarios
    pub fn nr_scenarios() -> Vec<TestScenario> {
        vec![
            TestScenario::nr_mmwave_mobility(),
            TestScenario::nr_network_slicing(),
            TestScenario::nr_carrier_aggregation_test(),
            TestScenario::nr_beamforming_interference(),
        ]
    }

    /// Get all available scenarios
    pub fn all_scenarios() -> Vec<TestScenario> {
        let mut scenarios = Self::basic_scenarios();
        scenarios.extend(Self::cellular_scenarios());
        scenarios.extend(Self::multi_link_scenarios());
        scenarios.extend(Self::nr_scenarios());
        scenarios
    }
}