//! Behavioral tests for the dynbitrate element
//!
//! These tests use the test harness encoder_stub and riststats_mock to simulate
//! network conditions and verify that dynbitrate adjusts the encoder bitrate
//! according to loss rate and RTT, obeying step-size, bounds, deadband and
//! rate limiting.

use gst::prelude::*;
use gstreamer as gst;

use serial_test::serial;

// Test helpers
use gstristelements::testing::{get_property, init_for_tests, wait_for_state_change};

// Test harness types (available under the default `test-plugin` feature)
#[cfg(feature = "test-plugin")]
use gstristelements::RistStatsMock;

// Convenience to create a basic pipeline with encoder_stub -> dynbitrate -> fakesink
#[cfg(feature = "test-plugin")]
fn make_pipeline_with_dynbitrate() -> (gst::Pipeline, gst::Element, gst::Element, RistStatsMock) {
    init_for_tests();

    // Elements
    let encoder = gstristelements::testing::create_encoder_stub(Some(5000)); // 5000 kbps start
    let dynbitrate = gstristelements::testing::create_dynbitrate();
    let sink = gstristelements::testing::create_fake_sink();

    // Use RIST stats mock as the RIST element dynbitrate reads from
    let rist_elem = gstristelements::testing::create_riststats_mock(None, None);
    let rist_mock = rist_elem
        .clone()
        .downcast::<RistStatsMock>()
        .expect("riststats_mock type");

    // Configure dynbitrate
    dynbitrate.set_property("encoder", &encoder);
    dynbitrate.set_property("rist", &rist_elem);
    dynbitrate.set_property("min-kbps", 1000u32);
    dynbitrate.set_property("max-kbps", 8000u32);
    dynbitrate.set_property("step-kbps", 500u32);
    dynbitrate.set_property("target-loss-pct", 1.0f64); // 1% target
    dynbitrate.set_property("min-rtx-rtt-ms", 40u64);

    // Build pipeline
    let pipeline = gst::Pipeline::new();
    pipeline
        .add_many([&encoder, &dynbitrate, &sink])
        .expect("add elements");
    gst::Element::link_many([&encoder, &dynbitrate, &sink]).expect("link chain");

    (pipeline, encoder, dynbitrate, rist_mock)
}

#[cfg(feature = "test-plugin")]
fn run_mainloop_ms(ms: u64) {
    // Pump the default GLib main context where timeout_add_local registered
    let ctx = glib::MainContext::default();
    let _guard = ctx.acquire().expect("acquire main context");
    let end = std::time::Instant::now() + std::time::Duration::from_millis(ms);
    while std::time::Instant::now() < end {
        // Drain all pending events without blocking, then sleep briefly.
        while ctx.iteration(false) {}
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[cfg(feature = "test-plugin")]
fn clean_shutdown(pipeline: gst::Pipeline) {
    // Stop pipeline and allow GLib to dispatch pending timeouts so they can detach
    let _ = pipeline.set_state(gst::State::Null);
    // Drop strong refs to help Weak<dynbitrate> fail on next tick
    drop(pipeline);
    // Give GLib a small window to process and remove sources on this same thread
    run_mainloop_ms(150);
}

#[test]
#[serial]
#[cfg(feature = "test-plugin")]
fn test_decrease_bitrate_on_high_loss_or_rtt() {
    let (pipeline, encoder, _dynb, rist_mock) = make_pipeline_with_dynbitrate();

    // Prime stats: one session with significant retransmissions and high RTT
    rist_mock.set_sessions(1);
    // 2000 original, 200 retrans -> 9.1% rtx rate; RTT 120ms (> 40ms)
    rist_mock.tick(&[2000], &[200], &[120]);

    // Start pipeline and wait for dynbitrate timer (750ms) and rate-limit (1200ms)
    wait_for_state_change(&pipeline, gst::State::Playing, 5).expect("playing");

    let start_br: u32 = get_property(&encoder, "bitrate").unwrap();

    // Let dynbitrate timer (750ms) fire and apply first adjustment immediately
    run_mainloop_ms(1600);

    let new_br: u32 = get_property(&encoder, "bitrate").unwrap();
    assert!(
        new_br == start_br.saturating_sub(500).max(1000),
        "Expected decrease by step (start={}, new={})",
        start_br,
        new_br
    );
    clean_shutdown(pipeline);
}

#[test]
#[serial]
#[cfg(feature = "test-plugin")]
fn test_increase_bitrate_on_good_conditions() {
    let (pipeline, encoder, _dynb, rist_mock) = make_pipeline_with_dynbitrate();

    // Prime stats: one session, no retransmissions, low RTT
    rist_mock.set_sessions(1);
    // 2000 original, 0 retrans -> 0% rtx; RTT 20ms (< 0.8 * 40ms)
    rist_mock.tick(&[2000], &[0], &[20]);

    wait_for_state_change(&pipeline, gst::State::Playing, 5).expect("playing");

    let start_br: u32 = get_property(&encoder, "bitrate").unwrap();

    run_mainloop_ms(1600);

    let new_br: u32 = get_property(&encoder, "bitrate").unwrap();
    assert!(
        new_br == (start_br + 500).min(8000),
        "Expected increase by step (start={}, new={})",
        start_br,
        new_br
    );
    clean_shutdown(pipeline);
}

#[test]
#[serial]
#[cfg(feature = "test-plugin")]
fn test_deadband_no_change_near_target_loss() {
    let (pipeline, encoder, _dynb, rist_mock) = make_pipeline_with_dynbitrate();

    // Target loss is 1.0%, deadband ±0.1%. Simulate ~1.0% exactly.
    rist_mock.set_sessions(1);
    // 10000 original, 100 retrans -> 1.0% rtx; RTT nominal
    rist_mock.tick(&[10_000], &[100], &[30]);

    wait_for_state_change(&pipeline, gst::State::Playing, 5).expect("playing");

    let start_br: u32 = get_property(&encoder, "bitrate").unwrap();

    run_mainloop_ms(1600);

    let new_br: u32 = get_property(&encoder, "bitrate").unwrap();
    assert_eq!(new_br, start_br, "Bitrate should remain stable in deadband");
    clean_shutdown(pipeline);
}

#[test]
#[serial]
#[cfg(feature = "test-plugin")]
fn test_rate_limiting_between_adjustments() {
    let (pipeline, encoder, _dynb, rist_mock) = make_pipeline_with_dynbitrate();

    rist_mock.set_sessions(1);
    // Force repeated decrease conditions
    rist_mock.tick(&[2000], &[200], &[120]);

    wait_for_state_change(&pipeline, gst::State::Playing, 5).expect("playing");
    let first: u32 = get_property(&encoder, "bitrate").unwrap();

    // Allow first change
    run_mainloop_ms(1600);
    let after_first: u32 = get_property(&encoder, "bitrate").unwrap();
    assert!(after_first < first, "Should have decreased once");

    // Not enough time for a second change (less than 1.2s since last change).
    // Run only ~0.5s to avoid crossing the next 750ms timer tick boundary
    // that could occur >1.2s after the previous change.
    run_mainloop_ms(500);
    let mid: u32 = get_property(&encoder, "bitrate").unwrap();
    assert_eq!(mid, after_first, "No second change due to rate limiting");

    // Now wait sufficiently for a second change
    run_mainloop_ms(1600);
    let after_second: u32 = get_property(&encoder, "bitrate").unwrap();
    assert!(
        after_second < after_first,
        "Should have decreased again after rate window"
    );
    clean_shutdown(pipeline);
}

#[test]
#[serial]
#[cfg(feature = "test-plugin")]
fn test_bounds_respected_at_min_max() {
    let (pipeline, encoder, dynb, rist_mock) = make_pipeline_with_dynbitrate();

    // Speed up convergence by increasing step size for this test
    dynb.set_property("step-kbps", 4000u32);

    // Drive bitrate down to min and ensure it doesn't go below
    rist_mock.set_sessions(1);
    // Strong decrease condition
    rist_mock.tick(&[5000], &[2000], &[200]);

    wait_for_state_change(&pipeline, gst::State::Playing, 5).expect("playing");

    // Run enough for two adjustments to hit min (5000->1000 with step 4000)
    run_mainloop_ms(4000);
    let at_min: u32 = get_property(&encoder, "bitrate").unwrap();
    assert_eq!(at_min, 1000, "Should clamp at min bitrate");

    // Reset counters to simulate a fresh window, then improve network to allow increases up to max
    rist_mock.set_sessions(1); // resets totals
    rist_mock.tick(&[5000], &[0], &[20]);
    // Run enough for two adjustments to reach max (1000->5000->8000)
    run_mainloop_ms(5000);
    let at_max: u32 = get_property(&encoder, "bitrate").unwrap();
    assert_eq!(at_max, 8000, "Should clamp at max bitrate");
    clean_shutdown(pipeline);
}
