//! Integration tests module
//!
//! Tests that involve GStreamer pipelines, element interactions,
//! and cross-component system behavior.

mod dynbitrate_behavior;
mod element_integration;
mod error_recovery;
mod keyframe_duplication;
mod metrics_debug;
mod metrics_export;
mod network_integration;
mod pad_removal_simple;
mod pipeline_tests;
mod property_debug;
mod thread_safety;
