//! Element pad semantics and event handling tests
//!
//! These tests verify that the dispatcher properly handles GStreamer events,
//! caps negotiation, and pad lifecycle management.

use gst::prelude::*;
use gstreamer as gst;
use gstristelements::testing::*;
use integration_tests::element_pad_semantics::*;

// Implement the testing provider trait for rist-elements
struct RistElementsTestingProvider;

impl DispatcherTestingProvider for RistElementsTestingProvider {
    fn create_dispatcher(weights: Option<&[f32]>) -> gst::Element {
        create_dispatcher(weights)
    }

    fn create_fake_sink() -> gst::Element {
        create_fake_sink()
    }

    fn create_counter_sink() -> gst::Element {
        create_counter_sink()
    }

    fn create_test_source() -> gst::Element {
        create_test_source()
    }

    fn init_for_tests() {
        init_for_tests()
    }

    fn wait_for_state_change(pipeline: &gst::Pipeline, state: gst::State, timeout_secs: u32) -> Result<(), gst::StateChangeError> {
        wait_for_state_change(pipeline, state, timeout_secs)
    }

    fn get_property<T>(element: &gst::Element, name: &str) -> Result<T, glib::Error> 
    where
        T: glib::value::FromValue + 'static
    {
        get_property(element, name)
    }

    fn run_pipeline_for_duration(pipeline: &gst::Pipeline, duration_secs: u32) -> Result<(), Box<dyn std::error::Error>> {
        run_pipeline_for_duration(pipeline, duration_secs)
    }
}

#[test]
fn test_caps_negotiation_and_proxying() {
    test_caps_negotiation_and_proxying::<RistElementsTestingProvider>();
}

#[test]
fn test_eos_event_fanout() {
    test_eos_event_fanout::<RistElementsTestingProvider>();
}

#[test]
fn test_flush_event_handling() {
    test_flush_event_handling::<RistElementsTestingProvider>();
}

#[test]
fn test_sticky_events_replay() {
    test_sticky_events_replay::<RistElementsTestingProvider>();
}

#[test]
fn test_pad_removal_and_cleanup() {
    test_pad_removal_and_cleanup::<RistElementsTestingProvider>();
}
