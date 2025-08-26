//! Cellular profiles and rate variation helpers
//!
//! Provides LTE/5G light/heavy profiles with realistic delay/jitter correlation,
//! bursty loss (via netem gemodel), reordering, and optional corruption.
//! Also includes rate variation patterns (random walk, sinusoid) applied to HTB.

#[derive(Clone, Debug)]
pub enum RadioAccessTechnology {
    Lte,
    Nr5g,
}

#[derive(Clone, Debug)]
pub enum LoadProfile {
    Light,
    Heavy,
}

#[derive(Clone, Debug)]
pub enum LossModel {
    /// Random loss with correlation percentage
    Random { pct: f32, corr_pct: u32 },
    /// Gilbert-Elliot model (gemodel) parameters
    Gemodel {
        p_enter_bad: f32,
        r_leave_bad: f32,
        bad_loss: f32,  // 1-h in netem gemodel
        good_loss: f32, // 1-k in netem gemodel
    },
}

#[derive(Clone, Debug)]
pub struct CellularProfile {
    pub rat: RadioAccessTechnology,
    pub load: LoadProfile,
    /// Target shaper rate (kbit)
    pub rate_kbit: u32,
    /// Delay base (ms)
    pub delay_ms: u32,
    /// Jitter (ms)
    pub jitter_ms: u32,
    /// Jitter correlation (%)
    pub corr_pct: u32,
    pub loss: LossModel,
    /// Reorder percent (0.0-100.0)
    pub reorder_pct: f32,
    /// Reorder correlation percent
    pub reorder_corr_pct: u32,
    /// Duplicate percent
    pub duplicate_pct: f32,
    /// Optional corruption percent (0 if not used)
    pub corrupt_pct: f32,
}

impl CellularProfile {
    pub fn lte_light() -> Self {
        Self {
            rat: RadioAccessTechnology::Lte,
            load: LoadProfile::Light,
            rate_kbit: 650,
            delay_ms: 70,
            jitter_ms: 10,
            corr_pct: 25,
            loss: LossModel::Gemodel {
                p_enter_bad: 0.006,
                r_leave_bad: 0.33,
                bad_loss: 0.5,
                good_loss: 0.0005,
            },
            reorder_pct: 0.2,
            reorder_corr_pct: 20,
            duplicate_pct: 0.0,
            corrupt_pct: 0.0,
        }
    }

    pub fn lte_heavy() -> Self {
        Self {
            rat: RadioAccessTechnology::Lte,
            load: LoadProfile::Heavy,
            rate_kbit: 1000,
            delay_ms: 150,
            jitter_ms: 25,
            corr_pct: 30,
            loss: LossModel::Gemodel {
                p_enter_bad: 0.013,
                r_leave_bad: 0.15,
                bad_loss: 0.6,
                good_loss: 0.001,
            },
            reorder_pct: 0.5,
            reorder_corr_pct: 30,
            duplicate_pct: 0.0,
            corrupt_pct: 0.02,
        }
    }

    pub fn nr5g_light() -> Self {
        Self {
            rat: RadioAccessTechnology::Nr5g,
            load: LoadProfile::Light,
            rate_kbit: 1150,
            delay_ms: 45,
            jitter_ms: 7,
            corr_pct: 25,
            loss: LossModel::Gemodel {
                p_enter_bad: 0.0039,
                r_leave_bad: 0.40,
                bad_loss: 0.5,
                good_loss: 0.0002,
            },
            reorder_pct: 0.1,
            reorder_corr_pct: 15,
            duplicate_pct: 0.0,
            corrupt_pct: 0.0,
        }
    }

    pub fn nr5g_heavy() -> Self {
        Self {
            rat: RadioAccessTechnology::Nr5g,
            load: LoadProfile::Heavy,
            rate_kbit: 1700,
            delay_ms: 110,
            jitter_ms: 18,
            corr_pct: 30,
            loss: LossModel::Gemodel {
                p_enter_bad: 0.0067,
                r_leave_bad: 0.20,
                bad_loss: 0.6,
                good_loss: 0.0005,
            },
            reorder_pct: 0.3,
            reorder_corr_pct: 25,
            duplicate_pct: 0.0,
            corrupt_pct: 0.0,
        }
    }
}

#[derive(Clone, Debug)]
pub enum RateVariation {
    /// Random walk around target with bounds and step clamp
    RandomWalk {
        target_kbit: u32,
        spread_kbit: u32,
        step_kbit: u32,
        period_ms: u64,
    },
    /// Sinusoidal variation around a target
    Sinusoid {
        target_kbit: u32,
        amp_kbit: u32,
        period_secs: u64,
    },
}
