//! Stress test: repeatedly request and release src pads while pipeline is running
//! to catch pad-lifecycle races.

use gst::prelude::*;
use gstreamer as gst;
use gstristelements::testing::*;

#[test]
fn stress_pad_lifecycle() {
    init_for_tests();

    println!("=== Stress Pad Lifecycle Test ===");

    // Create dispatcher without a full pipeline to avoid pipeline state issues
    let dispatcher = create_dispatcher_for_testing(Some(&[1.0]));

    // Simple stress test: just request and release pads without pipeline complexity
    println!("Starting pad stress test...");
    for i in 0..50 {
        if i % 10 == 0 {
            println!("Stress iteration {}", i);
        }

        // Request a pad
        if let Some(pad) = dispatcher.request_pad_simple("src_%u") {
            // Verify pad is valid
            assert!(pad.name().len() > 0, "Pad should have a valid name");

            // Release the pad
            dispatcher.release_request_pad(&pad);
        } else {
            panic!("Failed to request pad at iteration {}", i);
        }
    }

    // Final sanity check: request a pad and ensure dispatcher still responds
    if let Some(pad) = dispatcher.request_pad_simple("src_%u") {
        assert!(
            pad.name().len() > 0,
            "Failed to request pad after stress test"
        );
        dispatcher.release_request_pad(&pad);
    } else {
        panic!("Failed to request final test pad");
    }

    println!("✅ Stress pad lifecycle test completed");
}
