pub mod metrics;
pub mod plots;
pub mod pipelines;
pub mod scenarios;
pub mod util;
pub mod emulation;
pub mod weights;

// Export test registration from your plugin test feature
pub fn register_everything_for_tests() {
    use gstreamer as gst;
    
    let _ = gst::init();
    gstristsmart::register_for_tests();
}
