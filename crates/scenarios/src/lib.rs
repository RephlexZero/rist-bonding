//! Network scenario definitions and presets for RIST testbench
//! 
//! This crate provides data models for network impairments and realistic
//! 4G/5G behavior presets that can be used by both the netns-testbench
//! and the legacy netlink-sim backend.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

/// One direction of a link (TX->RX or RX->TX)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DirectionSpec {
    /// Base delay in milliseconds
    pub base_delay_ms: u32,
    /// Jitter in milliseconds (standard deviation)
    pub jitter_ms: u32,
    /// Random packet loss percentage (0.0-1.0)
    pub loss_pct: f32,
    /// Correlation for bursty loss (0.0-1.0, 0 = independent, 1 = highly correlated)
    pub loss_burst_corr: f32,
    /// Packet reordering percentage (0.0-1.0)
    pub reorder_pct: f32,
    /// Packet duplication percentage (0.0-1.0)
    pub duplicate_pct: f32,
    /// Average capacity in kbps
    pub rate_kbps: u32,
    /// Maximum transmission unit (bytes)
    pub mtu: Option<u32>,
}

impl DirectionSpec {
    /// Create a good quality direction
    pub fn good() -> Self {
        Self {
            base_delay_ms: 5,
            jitter_ms: 1,
            loss_pct: 0.00001,
            loss_burst_corr: 0.0,
            reorder_pct: 0.0,
            duplicate_pct: 0.0,
            rate_kbps: 50000, // 50 Mbps
            mtu: Some(1500),
        }
    }

    /// Create a typical internet direction
    pub fn typical() -> Self {
        Self {
            base_delay_ms: 20,
            jitter_ms: 5,
            loss_pct: 0.001,
            loss_burst_corr: 0.1,
            reorder_pct: 0.002,
            duplicate_pct: 0.0,
            rate_kbps: 20000, // 20 Mbps
            mtu: Some(1500),
        }
    }

    /// Create a poor quality direction
    pub fn poor() -> Self {
        Self {
            base_delay_ms: 100,
            jitter_ms: 25,
            loss_pct: 0.02,
            loss_burst_corr: 0.3,
            reorder_pct: 0.01,
            duplicate_pct: 0.001,
            rate_kbps: 2000, // 2 Mbps
            mtu: Some(1400),
        }
    }

    /// Create LTE uplink characteristics
    pub fn lte_uplink() -> Self {
        Self {
            base_delay_ms: 40,
            jitter_ms: 15,
            loss_pct: 0.003,
            loss_burst_corr: 0.2,
            reorder_pct: 0.005,
            duplicate_pct: 0.0,
            rate_kbps: 5000, // 5 Mbps uplink
            mtu: Some(1358), // Common for LTE
        }
    }

    /// Create LTE downlink characteristics
    pub fn lte_downlink() -> Self {
        Self {
            base_delay_ms: 35,
            jitter_ms: 12,
            loss_pct: 0.002,
            loss_burst_corr: 0.15,
            reorder_pct: 0.003,
            duplicate_pct: 0.0,
            rate_kbps: 25000, // 25 Mbps downlink
            mtu: Some(1358),
        }
    }

    /// Create 5G good quality characteristics
    pub fn nr_good() -> Self {
        Self {
            base_delay_ms: 15,
            jitter_ms: 8,
            loss_pct: 0.0005,
            loss_burst_corr: 0.1,
            reorder_pct: 0.001,
            duplicate_pct: 0.0,
            rate_kbps: 100000, // 100 Mbps
            mtu: Some(1500),
        }
    }

    /// Create 5G cell-edge characteristics
    pub fn nr_cell_edge() -> Self {
        Self {
            base_delay_ms: 25,
            jitter_ms: 15,
            loss_pct: 0.01,
            loss_burst_corr: 0.4,
            reorder_pct: 0.008,
            duplicate_pct: 0.0005,
            rate_kbps: 10000, // 10 Mbps at cell edge
            mtu: Some(1400),
        }
    }

    /// Create 5G mmWave characteristics (high-speed, short-range)
    pub fn nr_mmwave() -> Self {
        Self {
            base_delay_ms: 10, // Ultra-low latency
            jitter_ms: 5,
            loss_pct: 0.0001, // Very low loss when connected
            loss_burst_corr: 0.05,
            reorder_pct: 0.0005,
            duplicate_pct: 0.0,
            rate_kbps: 1000000, // 1 Gbps peak
            mtu: Some(1500),
        }
    }

    /// Create 5G mmWave with blockage (sudden signal loss)
    pub fn nr_mmwave_blocked() -> Self {
        Self {
            base_delay_ms: 200, // High delay due to handover/reconnection
            jitter_ms: 100,
            loss_pct: 0.5, // 50% loss during blockage
            loss_burst_corr: 0.95, // Highly correlated (blocked/unblocked states)
            reorder_pct: 0.1,
            duplicate_pct: 0.01,
            rate_kbps: 1000, // Very low rate during blockage
            mtu: Some(1400),
        }
    }

    /// Create 5G sub-6GHz characteristics (balanced coverage/speed)
    pub fn nr_sub6ghz() -> Self {
        Self {
            base_delay_ms: 20,
            jitter_ms: 10,
            loss_pct: 0.002,
            loss_burst_corr: 0.2,
            reorder_pct: 0.003,
            duplicate_pct: 0.0001,
            rate_kbps: 200000, // 200 Mbps typical
            mtu: Some(1500),
        }
    }

    /// Create 5G with carrier aggregation (CA) effects
    pub fn nr_carrier_aggregation() -> Self {
        Self {
            base_delay_ms: 18,
            jitter_ms: 12, // Slight increase due to scheduling complexity
            loss_pct: 0.001,
            loss_burst_corr: 0.15,
            reorder_pct: 0.005, // Higher reorder due to multiple carriers
            duplicate_pct: 0.0002,
            rate_kbps: 500000, // 500 Mbps with CA
            mtu: Some(1500),
        }
    }

    /// Create 5G with beamforming interference
    pub fn nr_beamforming_interference() -> Self {
        Self {
            base_delay_ms: 30,
            jitter_ms: 20, // Variable due to beam steering
            loss_pct: 0.008,
            loss_burst_corr: 0.6, // Interference comes in bursts
            reorder_pct: 0.01, // Beam switches can cause reordering
            duplicate_pct: 0.001,
            rate_kbps: 50000, // Reduced due to interference
            mtu: Some(1400),
        }
    }

    /// Create 5G uplink characteristics (typically asymmetric)
    pub fn nr_uplink() -> Self {
        Self {
            base_delay_ms: 25, // Slightly higher than downlink
            jitter_ms: 15,
            loss_pct: 0.004, // Higher loss in uplink
            loss_burst_corr: 0.3,
            reorder_pct: 0.006,
            duplicate_pct: 0.0003,
            rate_kbps: 50000, // 50 Mbps uplink
            mtu: Some(1500),
        }
    }

    /// Create 5G downlink characteristics
    pub fn nr_downlink() -> Self {
        Self {
            base_delay_ms: 15,
            jitter_ms: 10,
            loss_pct: 0.001,
            loss_burst_corr: 0.15,
            reorder_pct: 0.002,
            duplicate_pct: 0.0001,
            rate_kbps: 300000, // 300 Mbps downlink
            mtu: Some(1500),
        }
    }

        /// Create race car 4G USB modem characteristics - best case
    pub fn race_4g_strong() -> Self {
        Self {
            base_delay_ms: 45,
            jitter_ms: 20,
            loss_pct: 0.005,
            loss_burst_corr: 0.2,
            reorder_pct: 0.005,
            duplicate_pct: 0.0,
            rate_kbps: 2000, // 2 Mbps best case
            mtu: Some(1500),
        }
    }

    /// Create race car 4G USB modem characteristics - moderate signal
    pub fn race_4g_moderate() -> Self {
        Self {
            base_delay_ms: 65,
            jitter_ms: 35,
            loss_pct: 0.02,
            loss_burst_corr: 0.4,
            reorder_pct: 0.01,
            duplicate_pct: 0.001,
            rate_kbps: 1200, // 1.2 Mbps moderate
            mtu: Some(1500),
        }
    }

    /// Create race car 4G USB modem characteristics - weak signal
    pub fn race_4g_weak() -> Self {
        Self {
            base_delay_ms: 120,
            jitter_ms: 60,
            loss_pct: 0.05,
            loss_burst_corr: 0.6,
            reorder_pct: 0.02,
            duplicate_pct: 0.002,
            rate_kbps: 300, // 300 kbps worst case
            mtu: Some(1500),
        }
    }

    /// Create race car 5G USB modem characteristics - best case
    pub fn race_5g_strong() -> Self {
        Self {
            base_delay_ms: 25,
            jitter_ms: 15,
            loss_pct: 0.003,
            loss_burst_corr: 0.15,
            reorder_pct: 0.003,
            duplicate_pct: 0.0,
            rate_kbps: 2000, // 2 Mbps best case (USB modem limited)
            mtu: Some(1500),
        }
    }

    /// Create race car 5G USB modem characteristics - moderate signal
    pub fn race_5g_moderate() -> Self {
        Self {
            base_delay_ms: 35,
            jitter_ms: 25,
            loss_pct: 0.015,
            loss_burst_corr: 0.3,
            reorder_pct: 0.008,
            duplicate_pct: 0.001,
            rate_kbps: 1400, // 1.4 Mbps moderate
            mtu: Some(1500),
        }
    }

    /// Create race car 5G USB modem characteristics - weak signal
    pub fn race_5g_weak() -> Self {
        Self {
            base_delay_ms: 80,
            jitter_ms: 45,
            loss_pct: 0.04,
            loss_burst_corr: 0.5,
            reorder_pct: 0.015,
            duplicate_pct: 0.002,
            rate_kbps: 400, // 400 kbps worst case
            mtu: Some(1500),
        }
    }

    /// Create race car handover scenario - rapid cell tower change
    pub fn race_handover_spike() -> Self {
        Self {
            base_delay_ms: 200,
            jitter_ms: 100,
            loss_pct: 0.15, // High loss during handover
            loss_burst_corr: 0.8,
            reorder_pct: 0.05,
            duplicate_pct: 0.01,
            rate_kbps: 100, // Severely degraded during handover
            mtu: Some(1400), // Smaller MTU during instability
        }
    }
    pub fn nr_urllc() -> Self {
        Self {
            base_delay_ms: 3, // Ultra-low latency requirement
            jitter_ms: 1,
            loss_pct: 0.00001, // Ultra-reliable (99.999% reliability)
            loss_burst_corr: 0.01,
            reorder_pct: 0.0001,
            duplicate_pct: 0.0,
            rate_kbps: 10000, // Lower rate for ultra-reliable service
            mtu: Some(1500),
        }
    }

        /// Apply race car signal degradation (terrain/building blockage)
    pub fn with_race_blockage(mut self, severity: f32) -> Self {
        // Terrain/building blockage causes rate reduction and increased loss
        let degradation = 1.0 - (severity * 0.7); // Up to 70% degradation
        self.rate_kbps = (self.rate_kbps as f32 * degradation.max(0.15)) as u32; // Min 15% rate
        self.loss_pct = (self.loss_pct + severity * 0.03).min(0.1); // Max 10% loss
        self.base_delay_ms += (severity * 50.0) as u32; // Up to +50ms delay
        self.jitter_ms += (severity * 30.0) as u32; // Up to +30ms jitter
        self
    }

    /// Apply high-speed mobility effects (Doppler, rapid cell changes)
    pub fn with_mobility_effects(mut self, speed_factor: f32) -> Self {
        // High speed causes more jitter and handover issues
        self.jitter_ms += (speed_factor * 25.0) as u32; // More jitter at speed
        self.loss_burst_corr = (self.loss_burst_corr + speed_factor * 0.2).min(0.8);
        self.reorder_pct = (self.reorder_pct + speed_factor * 0.01).min(0.03);
        self
    }

    /// Apply USB modem limitations
    pub fn with_usb_constraints(mut self) -> Self {
        // USB modems have additional latency and processing overhead
        self.base_delay_ms += 15; // USB processing overhead
        self.jitter_ms += 10; // USB timing variations
        self.rate_kbps = self.rate_kbps.min(2500); // USB bandwidth ceiling
        self
    }
    pub fn nr_embb() -> Self {
        Self {
            base_delay_ms: 20,
            jitter_ms: 8,
            loss_pct: 0.001,
            loss_burst_corr: 0.1,
            reorder_pct: 0.002,
            duplicate_pct: 0.0001,
            rate_kbps: 800000, // 800 Mbps for enhanced broadband
            mtu: Some(1500),
        }
    }
    pub fn nr_mmtc() -> Self {
        Self {
            base_delay_ms: 100, // Higher latency tolerated
            jitter_ms: 50,
            loss_pct: 0.01, // Higher loss tolerated
            loss_burst_corr: 0.3,
            reorder_pct: 0.005,
            duplicate_pct: 0.001,
            rate_kbps: 1000, // Low rate for IoT devices
            mtu: Some(1200), // Smaller MTU for IoT
        }
    }

    /// Create satellite characteristics
    pub fn satellite() -> Self {
        Self {
            base_delay_ms: 300,
            jitter_ms: 50,
            loss_pct: 0.005,
            loss_burst_corr: 0.2,
            reorder_pct: 0.002,
            duplicate_pct: 0.0,
            rate_kbps: 5000, // 5 Mbps
            mtu: Some(1300), // Conservative for satellite
        }
    }

    /// Apply handover spike effects
    pub fn with_handover_spike(mut self) -> Self {
        self.base_delay_ms += 200; // Temporary RTT spike
        self.jitter_ms *= 3;
        self.loss_pct = (self.loss_pct * 10.0).min(0.1); // Temporary loss spike
        self.loss_burst_corr = 0.8; // Highly bursty during handover
        self.reorder_pct *= 5.0;
        self
    }

    /// Apply mmWave beam blockage effects (sudden signal loss/recovery)
    pub fn with_mmwave_blockage(mut self, blockage_severity: f32) -> Self {
        let severity = blockage_severity.clamp(0.0, 1.0);
        
        // Blockage causes sudden high loss and delay spikes
        self.loss_pct = (self.loss_pct + severity * 0.3).min(1.0); // Up to 30% additional loss
        self.loss_burst_corr = (self.loss_burst_corr + severity * 0.5).min(1.0); // Highly bursty
        self.base_delay_ms += (severity * 100.0) as u32; // Up to 100ms spike
        self.jitter_ms += (severity * 50.0) as u32;
        
        // Rate drops significantly during blockage
        self.rate_kbps = ((self.rate_kbps as f32) * (1.0 - severity * 0.9)) as u32;
        
        self
    }

    /// Apply beamforming steering effects
    pub fn with_beamforming_steering(mut self, steering_intensity: f32) -> Self {
        let intensity = steering_intensity.clamp(0.0, 1.0);
        
        // Beam steering causes variable delay and reordering
        self.jitter_ms += (intensity * 20.0) as u32;
        self.reorder_pct += intensity * 0.01; // Up to 1% reordering
        
        // Brief loss spikes during beam transitions
        self.loss_pct += intensity * 0.005;
        self.loss_burst_corr += intensity * 0.3;
        
        self
    }

    /// Apply carrier aggregation effects
    pub fn with_carrier_aggregation(mut self, ca_bands: u32) -> Self {
        let multiplier = (ca_bands as f32).max(1.0);
        
        // CA increases rate but can cause reordering due to different band delays
        self.rate_kbps = ((self.rate_kbps as f32) * multiplier) as u32;
        self.reorder_pct += (multiplier - 1.0) * 0.002; // Slight reordering increase per extra band
        self.jitter_ms += ((multiplier - 1.0) * 5.0) as u32; // Scheduling complexity
        
        self
    }

    /// Apply bufferbloat effects (queue buildup)
    pub fn with_bufferbloat(mut self, severity: f32) -> Self {
        // Bufferbloat increases delay proportionally and adds jitter
        let multiplier = 1.0 + severity * 5.0; // 1.0 to 6.0x
        self.base_delay_ms = ((self.base_delay_ms as f32) * multiplier) as u32;
        self.jitter_ms = ((self.jitter_ms as f32) * multiplier) as u32;
        self
    }
}

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
            initial_state: 1, // Start in moderate state
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
            (Duration::from_secs(15), handover.clone()),  // Handover event
            (Duration::from_secs(18), normal_5g.clone()), // Switch to 5G tower
            (Duration::from_secs(35), handover.clone()),  // Another handover
            (Duration::from_secs(38), normal_4g),         // Back to 4G
            (Duration::from_secs(55), handover),          // Final handover
            (Duration::from_secs(58), normal_5g),         // Settle on 5G
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
            (Duration::from_secs(0), pit_straight),     // Best signal at pit
            (Duration::from_secs(15), turn_complex.clone()), // Turns with blockage
            (Duration::from_secs(25), back_straight),   // Good signal on back straight
            (Duration::from_secs(45), turn_complex.clone()),    // More turns
            (Duration::from_secs(55), tunnel_section),  // Worst signal in tunnel
            (Duration::from_secs(65), turn_complex),    // Final turn complex
            // Loop repeats at 90s
        ])
    }
}

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
        metadata.insert("expected_behavior".to_string(), "weight_adaptation".to_string());

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
                    (Duration::from_secs(60), lte_edge.clone().with_handover_spike()),
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
                    (Duration::from_secs(30), DirectionSpec::nr_mmwave().with_mmwave_blockage(1.0)),
                    (Duration::from_secs(33), DirectionSpec::nr_mmwave()), // Quick recovery
                    (Duration::from_secs(60), DirectionSpec::nr_mmwave().with_mmwave_blockage(0.5)),
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
                    (Duration::from_secs(30), DirectionSpec::nr_sub6ghz().with_carrier_aggregation(2)),
                    (Duration::from_secs(60), DirectionSpec::nr_sub6ghz().with_carrier_aggregation(3)),
                    (Duration::from_secs(90), DirectionSpec::nr_carrier_aggregation()),
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

/// Utilities for scenario manipulation
pub mod utils {
    use super::*;

    /// Scale all rates in a direction by a factor
    pub fn scale_rate(mut spec: DirectionSpec, factor: f32) -> DirectionSpec {
        spec.rate_kbps = ((spec.rate_kbps as f32) * factor) as u32;
        spec
    }

    /// Increase loss in a direction by additive amount
    pub fn add_loss(mut spec: DirectionSpec, additional_loss: f32) -> DirectionSpec {
        spec.loss_pct = (spec.loss_pct + additional_loss).min(1.0);
        spec
    }

    /// Create a stepped degradation from good to poor over time
    pub fn create_degradation(
        good: DirectionSpec,
        poor: DirectionSpec,
        steps: u32,
        total_duration: Duration,
    ) -> Schedule {
        let mut schedule_steps = Vec::new();
        let step_duration = total_duration / steps;

        for i in 0..=steps {
            let t = (i as f32) / (steps as f32);
            let interpolated = interpolate_specs(&good, &poor, t);
            schedule_steps.push((step_duration * i, interpolated));
        }

        Schedule::Steps(schedule_steps)
    }

    /// Linearly interpolate between two DirectionSpecs
    fn interpolate_specs(a: &DirectionSpec, b: &DirectionSpec, t: f32) -> DirectionSpec {
        let t = t.clamp(0.0, 1.0);
        DirectionSpec {
            base_delay_ms: lerp(a.base_delay_ms as f32, b.base_delay_ms as f32, t) as u32,
            jitter_ms: lerp(a.jitter_ms as f32, b.jitter_ms as f32, t) as u32,
            loss_pct: lerp(a.loss_pct, b.loss_pct, t),
            loss_burst_corr: lerp(a.loss_burst_corr, b.loss_burst_corr, t),
            reorder_pct: lerp(a.reorder_pct, b.reorder_pct, t),
            duplicate_pct: lerp(a.duplicate_pct, b.duplicate_pct, t),
            rate_kbps: lerp(a.rate_kbps as f32, b.rate_kbps as f32, t) as u32,
            mtu: a.mtu.or(b.mtu), // Use first non-None MTU
        }
    }

    fn lerp(a: f32, b: f32, t: f32) -> f32 {
        a + (b - a) * t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_direction_presets() {
        let good = DirectionSpec::good();
        let poor = DirectionSpec::poor();
        
        assert!(good.rate_kbps > poor.rate_kbps);
        assert!(good.loss_pct < poor.loss_pct);
        assert!(good.base_delay_ms < poor.base_delay_ms);
    }

    #[test]
    fn test_scenario_builder() {
        let scenario = ScenarioBuilder::new("test")
            .description("Test scenario")
            .duration(Duration::from_secs(60))
            .metadata("type", "unit_test")
            .build();

        assert_eq!(scenario.name, "test");
        assert_eq!(scenario.duration_seconds, Some(60));
        assert_eq!(scenario.metadata.get("type"), Some(&"unit_test".to_string()));
    }

    #[test]
    fn test_presets() {
        let basic = Presets::basic_scenarios();
        assert!(!basic.is_empty());
        
        let all = Presets::all_scenarios();
        assert!(all.len() >= basic.len());
    }

    #[test]
    fn test_utils_scaling() {
        let spec = DirectionSpec::typical();
        let scaled = utils::scale_rate(spec.clone(), 2.0);
        
        assert_eq!(scaled.rate_kbps, spec.rate_kbps * 2);
    }
}