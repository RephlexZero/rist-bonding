//! Testing utilities and convenience functions for RIST elements
//!
//! This module provides convenient macros and functions to reduce test boilerplate
//! and make tests more readable and maintainable.
//!
//! The test harness elements are enabled by default, so you can simply run:
//! ```bash
//! cargo test
//! ```

#[cfg(feature = "test-plugin")]
use crate::test_harness::RistStatsMock;
use gst::prelude::*;
use gstreamer as gst;

/// Network simulation integration (requires the `netns-sim` feature)
#[cfg(feature = "netns-sim")]
pub mod network_sim {
    use gstreamer::prelude::*;
    use netns_testbench::{NetworkOrchestrator, TestScenario};
    use std::future::Future;
    use tokio::time::{timeout, Duration};

    /// Start a network simulation scenario for RIST testing
    pub async fn setup_network_scenario(
        scenario: TestScenario,
        rx_port: u16,
    ) -> Result<NetworkOrchestrator, Box<dyn std::error::Error>> {
        // Create orchestrator and start scenario
        let mut orchestrator = NetworkOrchestrator::new(42).await?;
        let _handle = orchestrator.start_scenario(scenario, rx_port).await?;
        Ok(orchestrator)
    }

    /// Setup dual-link bonding test environment
    pub async fn setup_bonding_test(
        rx_port: u16,
    ) -> Result<NetworkOrchestrator, Box<dyn std::error::Error>> {
        // Use bonding asymmetric scenario for bonding tests
        let scenario = TestScenario::bonding_asymmetric();
        setup_network_scenario(scenario, rx_port).await
    }

    /// Run a test with network simulation for a specific duration
    pub async fn run_test_with_network<F, Fut>(
        scenario: TestScenario,
        rx_port: u16,
        test_duration_secs: u64,
        test_fn: F,
    ) -> Result<(), Box<dyn std::error::Error>>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), String>> + Send + 'static,
    {
        let _orchestrator = setup_network_scenario(scenario, rx_port).await?;

        // Run the test with a timeout
        let test_future = test_fn();
        let result = timeout(Duration::from_secs(test_duration_secs), test_future).await;

        match result {
            Ok(test_result) => test_result.map_err(|e| e.into()),
            Err(_) => Err(format!("Test timed out after {} seconds", test_duration_secs).into()),
        }
    }

    /// Create preset network scenarios for common test cases
    pub fn get_test_scenarios() -> Vec<TestScenario> {
        vec![
            TestScenario::baseline_good(),
            TestScenario::degrading_network(),
            TestScenario::mobile_handover(),
            TestScenario::bonding_asymmetric(),
            TestScenario::nr_to_lte_handover(),
        ]
    }

    /// Helper to test RIST dispatcher behavior under different network conditions
    pub async fn test_dispatcher_with_network(
        weights: Option<&[f64]>,
        scenario: TestScenario,
        rx_port: u16,
        test_duration_secs: u64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let _orchestrator = setup_network_scenario(scenario, rx_port).await?;

        // Initialize RIST elements
        crate::testing::init_for_tests();

        let dispatcher = crate::testing::create_dispatcher_for_testing(weights);

        // Create a simple test pipeline
        let pipeline = gstreamer::Pipeline::new();
        let src = crate::testing::create_test_source();
        let sink = crate::testing::create_fake_sink();

        pipeline.add_many([&src, &dispatcher, &sink])?;
        gstreamer::Element::link_many([&src, &dispatcher, &sink])?;

        // Run the test
        crate::testing::run_pipeline_for_duration(&pipeline, test_duration_secs)?;

        Ok(())
    }
}

/// Initialize GStreamer and register all RIST elements for testing
/// This should be called at the start of each test
#[cfg(feature = "test-plugin")]
pub fn init_for_tests() {
    crate::register_for_tests();
}

/// Initialize GStreamer and register main RIST elements (without test harness)
/// This version is available without the test-plugin feature
#[cfg(not(feature = "test-plugin"))]
pub fn init_for_tests() {
    use gstreamer as gst;
    let _ = gst::init();
    // Register main elements with None plugin handle
    let _ = crate::dispatcher::register_static();
    let _ = crate::dynbitrate::register_static();
}

/// Create a mock RIST stats element with specified number of sessions
#[cfg(feature = "test-plugin")]
pub fn create_mock_stats(num_sessions: usize) -> RistStatsMock {
    let mock = gst::ElementFactory::make("riststats_mock")
        .build()
        .expect("Failed to create riststats_mock")
        .downcast::<RistStatsMock>()
        .unwrap();

    mock.set_sessions(num_sessions);
    mock
}

/// Create a RIST dispatcher element with specified weights
pub fn create_dispatcher(weights: Option<&[f64]>) -> gst::Element {
    let dispatcher = gst::ElementFactory::make("ristdispatcher")
        .build()
        .expect("Failed to create ristdispatcher");

    if let Some(w) = weights {
        let weights_json = serde_json::to_string(w).expect("Failed to serialize weights");
        dispatcher.set_property("weights", &weights_json);
    }

    dispatcher
}

/// Create a RIST dispatcher element configured for load balancing tests
pub fn create_dispatcher_for_testing(weights: Option<&[f64]>) -> gst::Element {
    let dispatcher = create_dispatcher(weights);

    // Configure for proper load balancing testing
    dispatcher.set_property("auto-balance", false);
    dispatcher.set_property("min-hold-ms", 0u64); // No hold time
    dispatcher.set_property("switch-threshold", 1.0); // No threshold
    dispatcher.set_property("health-warmup-ms", 0u64); // No warmup period

    dispatcher
}

/// Create a dynamic bitrate controller element
pub fn create_dynbitrate() -> gst::Element {
    gst::ElementFactory::make("dynbitrate")
        .build()
        .expect("Failed to create dynbitrate")
}

/// Create a counter sink element for testing buffer flow
#[cfg(feature = "test-plugin")]
pub fn create_counter_sink() -> gst::Element {
    gst::ElementFactory::make("counter_sink")
        .build()
        .expect("Failed to create counter_sink")
}

/// Create an encoder stub element for testing bitrate control
#[cfg(feature = "test-plugin")]
pub fn create_encoder_stub(initial_bitrate: Option<u32>) -> gst::Element {
    let encoder = gst::ElementFactory::make("encoder_stub")
        .build()
        .expect("Failed to create encoder_stub");

    if let Some(bitrate) = initial_bitrate {
        encoder.set_property("bitrate", bitrate);
    }

    encoder
}

/// Create a RIST stats mock element with specified quality and RTT
#[cfg(feature = "test-plugin")]
pub fn create_riststats_mock(quality: Option<f64>, rtt: Option<u32>) -> gst::Element {
    let mock = gst::ElementFactory::make("riststats_mock")
        .build()
        .expect("Failed to create riststats_mock");

    if let Some(q) = quality {
        mock.set_property("quality", q);
    }

    if let Some(r) = rtt {
        mock.set_property("rtt", r);
    }

    mock
}

/// Convenience macro for creating test pipelines with common elements
#[macro_export]
macro_rules! test_pipeline {
    ($name:ident, $($element:expr),* $(,)?) => {
        let $name = gst::Pipeline::new();
        $(
            $name.add($element).expect("Failed to add element to pipeline");
        )*
    };
}

/// Convenience macro for linking elements in a pipeline
#[macro_export]
macro_rules! link_elements {
    ($($element:expr),* $(,)?) => {
        gst::Element::link_many(&[$($element),*])
            .expect("Failed to link elements");
    };
}

/// Wait for a pipeline to reach a specific state with timeout
pub fn wait_for_state_change(
    pipeline: &gst::Pipeline,
    target_state: gst::State,
    timeout_secs: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    pipeline.set_state(target_state)?;

    let bus = pipeline.bus().unwrap();
    let timeout = gst::ClockTime::from_seconds(timeout_secs);

    match bus.timed_pop_filtered(
        Some(timeout),
        &[
            gst::MessageType::AsyncDone,
            gst::MessageType::StateChanged,
            gst::MessageType::Error,
        ],
    ) {
        Some(msg) => match msg.view() {
            gst::MessageView::Error(err) => Err(format!("Pipeline error: {}", err.error()).into()),
            gst::MessageView::AsyncDone(..) | gst::MessageView::StateChanged(..) => Ok(()),
            _ => Ok(()),
        },
        None => Err("Timeout waiting for state change".into()),
    }
}

/// Run a pipeline for a specified duration
pub fn run_pipeline_for_duration(
    pipeline: &gst::Pipeline,
    duration_secs: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::{thread, time::Duration};

    wait_for_state_change(pipeline, gst::State::Playing, 5)?;

    thread::sleep(Duration::from_secs(duration_secs));

    pipeline.set_state(gst::State::Null)?;
    Ok(())
}

/// Extract property value as specific type with better error handling
pub fn get_property<T>(
    element: &gst::Element,
    property: &str,
) -> Result<T, Box<dyn std::error::Error>>
where
    T: for<'a> gst::glib::value::FromValue<'a> + 'static,
{
    element
        .property_value(property)
        .get::<T>()
        .map_err(|e| format!("Failed to get property '{}': {}", property, e).into())
}

/// Create a simple test source element
pub fn create_test_source() -> gst::Element {
    gst::ElementFactory::make("audiotestsrc")
        .property("num-buffers", 100)
        .property("freq", 440.0)
        .build()
        .expect("Failed to create audiotestsrc")
}

/// Create a fake sink for testing
pub fn create_fake_sink() -> gst::Element {
    gst::ElementFactory::make("fakesink")
        .property("sync", false)
        .build()
        .expect("Failed to create fakesink")
}

/// Create a RIST sink for testing
pub fn create_rist_sink(address: &str) -> gst::Element {
    gst::ElementFactory::make("ristsink")
        .property("address", address)
        .property("port", 5000u32)
        .property("buffer-time", 200u32)
        .build()
        .expect("Failed to create ristsink")
}

/// Create a RIST source for testing
pub fn create_rist_source(address: &str) -> gst::Element {
    gst::ElementFactory::make("ristsrc")
        .property("address", address)
        .property("port", 5000u32)
        .property("buffer-time", 200u32)
        .build()
        .expect("Failed to create ristsrc")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_for_tests() {
        init_for_tests();

        // Verify main elements are available
        assert!(gst::ElementFactory::find("ristdispatcher").is_some());
        assert!(gst::ElementFactory::find("dynbitrate").is_some());
    }

    #[test]
    #[cfg(feature = "test-plugin")]
    fn test_test_harness_elements() {
        init_for_tests();

        // Verify test harness elements are available
        assert!(gst::ElementFactory::find("counter_sink").is_some());
        assert!(gst::ElementFactory::find("encoder_stub").is_some());
        assert!(gst::ElementFactory::find("riststats_mock").is_some());
    }

    #[test]
    fn test_create_dispatcher() {
        init_for_tests();

        let dispatcher = create_dispatcher(Some(&[0.5, 0.3, 0.2]));
        assert_eq!(dispatcher.factory().unwrap().name(), "ristdispatcher");
    }

    #[test]
    #[cfg(feature = "test-plugin")]
    fn test_create_mock_stats() {
        init_for_tests();

        let mock = create_mock_stats(3);
        assert_eq!(mock.factory().unwrap().name(), "riststats_mock");
    }
}
