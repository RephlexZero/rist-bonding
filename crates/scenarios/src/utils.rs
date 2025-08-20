//! Utilities for scenario manipulation
//!
//! This module provides utility functions for modifying and creating
//! network specifications dynamically.

use crate::direction::DirectionSpec;
use crate::schedule::Schedule;
use std::time::Duration;

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