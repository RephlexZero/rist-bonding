//! RIST Dispatcher module (refactored)
//! Public facade re-exporting the element type and registration helpers.

pub use self::element::{register, register_static, Dispatcher};

mod duplication;
mod element;
mod health;
mod metrics;
mod pads;
mod props;
mod scheduler;
mod state;
mod stats;
mod strategy;
mod timers;
