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
    gstristsmart,
    env!("CARGO_PKG_DESCRIPTION"),
    plugin_init,
    env!("CARGO_PKG_VERSION"),
    "MIT OR Apache-2.0",
    env!("CARGO_PKG_NAME"),
    "gst-rist-smart",
    "https://github.com/user/repo",
    "2025-01-01"
);
