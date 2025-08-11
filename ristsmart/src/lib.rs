use gst::glib;
use gstreamer as gst;

mod dispatcher;
mod dynbitrate;

// Export public types
pub use crate::dispatcher::Dispatcher;
pub use crate::dynbitrate::DynBitrate;

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
    // Register elements with None plugin handle
    let _ = dispatcher::register_static();
    let _ = dynbitrate::register_static();
}
