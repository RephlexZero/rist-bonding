//! Network direction specifications with quality presets
//!
//! This module provides DirectionSpec which models one direction of a network link
//! (TX->RX or RX->TX) with comprehensive impairment parameters and realistic presets
//! for different network types (4G/5G, fixed broadband, satellite, etc.).

use serde::{Deserialize, Serialize};

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
            loss_pct: 0.5,         // 50% loss during blockage
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
            reorder_pct: 0.01,    // Beam switches can cause reordering
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
            rate_kbps: 100,  // Severely degraded during handover
            mtu: Some(1400), // Smaller MTU during instability
        }
    }

    /// Create 5G URLLC (Ultra-Reliable Low-Latency Communication) characteristics
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

    /// Create 5G eMBB (enhanced Mobile Broadband) characteristics
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

    /// Create 5G mMTC (massive Machine Type Communication) characteristics
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