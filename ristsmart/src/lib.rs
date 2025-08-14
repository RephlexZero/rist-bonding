use gst::glib;
use gstreamer as gst;

mod dispatcher;
mod dynbitrate;

// Test harness (only compiled with test-plugin feature)
#[cfg(feature = "test-plugin")]
mod test_harness;

// Testing utilities (always available)
pub mod testing;

// Export public types
pub use crate::dispatcher::Dispatcher;
pub use crate::dynbitrate::DynBitrate;

// Export test harness types when feature is enabled
#[cfg(feature = "test-plugin")]
pub use crate::test_harness::RistStatsMock;

// Register plugin
fn plugin_init(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    dispatcher::register(plugin)?;
    dynbitrate::register(plugin)?;
    Ok(())
}

gst::plugin_define!(
    ristsmart,
    env!("CARGO_PKG_DESCRIPTION"),
    plugin_init,
    env!("CARGO_PKG_VERSION"),
    "MIT",
    env!("CARGO_PKG_NAME"),
    "gst-rist-smart",
    "https://github.com/RephlexZero/rist-bonding",
    "2025-01-01"
);

// Static registration helper for tests: directly register elements without a Plugin
#[cfg(feature = "test-plugin")]
pub fn register_for_tests() {
    let _ = gst::init();
    // Register main elements with None plugin handle
    let _ = dispatcher::register_static();
    let _ = dynbitrate::register_static();
    
    // Register test harness elements
    if let Err(e) = test_harness::register_test_elements() {
        eprintln!("Failed to register test harness elements: {}", e);
    } else {
        // Verify registration
        if gst::ElementFactory::find("ristdispatcher").is_some() {
            println!("✓ ristdispatcher registered successfully");
        }
        if gst::ElementFactory::find("dynbitrate").is_some() {
            println!("✓ dynbitrate registered successfully");
        }
        if gst::ElementFactory::find("counter_sink").is_some() {
            println!("✓ counter_sink registered successfully");
        }
        if gst::ElementFactory::find("encoder_stub").is_some() {
            println!("✓ encoder_stub registered successfully");
        }
        if gst::ElementFactory::find("riststats_mock").is_some() {
            println!("✓ riststats_mock registered successfully");
        }
    }
}
