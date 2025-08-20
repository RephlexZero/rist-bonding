//! Time-varying network schedules for dynamic testing scenarios
//!
//! This module provides Schedule enum for describing how network conditions
//! change over time, including constant conditions, stepped changes, 
//! Markov chains, and trace replay capabilities.

use crate::direction::DirectionSpec;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// Time-varying schedule for a direction
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Schedule {
    /// Constant parameters throughout the session
    Constant(DirectionSpec),
    /// Piecewise constant steps: (time_from_start, new_params)
    Steps(Vec<(Duration, DirectionSpec)>),
    /// Markov chain with state transitions
    Markov {
        states: Vec<DirectionSpec>,
        /// Transition probability matrix: p[i][j] = P(i -> j)
        transition_matrix: Vec<Vec<f32>>,
        /// Initial state index
        initial_state: usize,
        /// Mean time between state transitions
        mean_dwell_time: Duration,
    },
    /// Replay from trace file (CSV/JSON)
    Replay { path: PathBuf },
}

impl Schedule {
    /// Create a simple degradation schedule: good -> poor -> recovery
    pub fn degradation_cycle(good: DirectionSpec, poor: DirectionSpec) -> Self {
        Schedule::Steps(vec![
            (Duration::from_secs(0), good.clone()),
            (Duration::from_secs(30), poor),
            (Duration::from_secs(90), good),
        ])
    }

    /// Create a handover simulation: normal -> spike -> recovery
    pub fn handover_simulation(normal: DirectionSpec) -> Self {
        let spike = normal.clone().with_handover_spike();
        Schedule::Steps(vec![
            (Duration::from_secs(0), normal.clone()),
            (Duration::from_secs(60), spike),
            (Duration::from_secs(65), normal), // Quick recovery
        ])
    }

    /// Create a Markov chain for bursty conditions
    pub fn bursty_markov(good: DirectionSpec, poor: DirectionSpec) -> Self {
        Schedule::Markov {
            states: vec![good, poor],
            // Stay in good 90%, poor 70% of the time
            transition_matrix: vec![
                vec![0.9, 0.1], // good -> good, good -> poor
                vec![0.3, 0.7], // poor -> good, poor -> poor
            ],
            initial_state: 0,
            mean_dwell_time: Duration::from_secs(10),
        }
    }

    /// Create a race car 4G Markov chain - realistic signal variations
    pub fn race_4g_markov() -> Self {
        let strong = DirectionSpec::race_4g_strong().with_usb_constraints();
        let moderate = DirectionSpec::race_4g_moderate().with_usb_constraints();
        let weak = DirectionSpec::race_4g_weak().with_usb_constraints();

        Schedule::Markov {
            states: vec![strong, moderate, weak],
            // Race conditions: frequently changing signal strength
            transition_matrix: vec![
                vec![0.70, 0.25, 0.05], // strong -> mostly stay strong, some moderate
                vec![0.30, 0.50, 0.20], // moderate -> can improve or degrade
                vec![0.10, 0.40, 0.50], // weak -> mostly stay weak or improve to moderate
            ],
            initial_state: 1,                        // Start in moderate state
            mean_dwell_time: Duration::from_secs(8), // 8s average - fast changes at race speeds
        }
    }

    /// Create a race car 5G Markov chain - better but still mobile limited
    pub fn race_5g_markov() -> Self {
        let strong = DirectionSpec::race_5g_strong().with_usb_constraints();
        let moderate = DirectionSpec::race_5g_moderate().with_usb_constraints();
        let weak = DirectionSpec::race_5g_weak().with_usb_constraints();

        Schedule::Markov {
            states: vec![strong, moderate, weak],
            // 5G is better but still affected by race mobility
            transition_matrix: vec![
                vec![0.80, 0.18, 0.02], // strong -> better stability than 4G
                vec![0.40, 0.50, 0.10], // moderate -> better recovery than 4G
                vec![0.20, 0.50, 0.30], // weak -> better recovery than 4G
            ],
            initial_state: 0, // Start in strong state (5G advantage)
            mean_dwell_time: Duration::from_secs(12), // 12s average - slightly more stable
        }
    }

    /// Create a race car handover simulation - frequent cell changes
    pub fn race_handover_pattern() -> Self {
        let normal_4g = DirectionSpec::race_4g_moderate().with_usb_constraints();
        let normal_5g = DirectionSpec::race_5g_moderate().with_usb_constraints();
        let handover = DirectionSpec::race_handover_spike();

        Schedule::Steps(vec![
            (Duration::from_secs(0), normal_4g.clone()),
            (Duration::from_secs(15), handover.clone()), // Handover event
            (Duration::from_secs(18), normal_5g.clone()), // Switch to 5G tower
            (Duration::from_secs(35), handover.clone()), // Another handover
            (Duration::from_secs(38), normal_4g),        // Back to 4G
            (Duration::from_secs(55), handover),         // Final handover
            (Duration::from_secs(58), normal_5g),        // Settle on 5G
        ])
    }

    /// Create a race track signal degradation pattern
    pub fn race_track_circuit() -> Self {
        let pit_straight = DirectionSpec::race_5g_strong().with_usb_constraints();
        let back_straight = DirectionSpec::race_4g_strong().with_usb_constraints();
        let turn_complex = DirectionSpec::race_4g_moderate().with_race_blockage(0.4);
        let tunnel_section = DirectionSpec::race_4g_weak().with_race_blockage(0.8);

        // Simulate a 90-second lap with varying signal conditions
        Schedule::Steps(vec![
            (Duration::from_secs(0), pit_straight), // Best signal at pit
            (Duration::from_secs(15), turn_complex.clone()), // Turns with blockage
            (Duration::from_secs(25), back_straight), // Good signal on back straight
            (Duration::from_secs(45), turn_complex.clone()), // More turns
            (Duration::from_secs(55), tunnel_section), // Worst signal in tunnel
            (Duration::from_secs(65), turn_complex), // Final turn complex
                                                    // Loop repeats at 90s
        ])
    }
}