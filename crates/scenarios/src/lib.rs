//! Network scenario definitions and presets for RIST testbench
//!
//! This crate provides data models for network impairments and realistic
//! 4G/5G behavior presets that can be used by both the netns-testbench
//! and the netns-testbench backend.

pub mod builder;
pub mod direction;
pub mod link;
pub mod presets;
pub mod scenario;
pub mod schedule;
pub mod utils;

// Re-export main types for convenience
pub use builder::ScenarioBuilder;
pub use direction::DirectionSpec;
pub use link::LinkSpec;
pub use presets::Presets;
pub use scenario::TestScenario;
pub use schedule::Schedule;