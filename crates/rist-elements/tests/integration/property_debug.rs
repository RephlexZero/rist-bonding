//! Debug property type issues

use gst::prelude::*;
use gstreamer as gst;
use gstristelements::testing::*;

#[test]
fn debug_counter_sink_properties() {
    init_for_tests();

    let counter = create_counter_sink();

    // Let's inspect what properties are actually available and their types
    let klass = counter.element_class();
    let properties = klass.list_properties();

    for prop in properties {
        println!(
            "Property '{}': Type={:?}, Value type={:?}",
            prop.name(),
            prop.type_(),
            prop.value_type()
        );
    }

    // Try to get the count property and see what type it actually returns
    match counter.property_value("count").type_() {
        t => println!("count property actual GType: {:?}", t),
    }

    // Try to get it as different types to see what works
    match counter.property_value("count").get::<bool>() {
        Ok(val) => println!("count as bool: {}", val),
        Err(e) => println!("count as bool failed: {}", e),
    }
    match counter.property_value("count").get::<u64>() {
        Ok(val) => println!("count as u64: {}", val),
        Err(e) => println!("count as u64 failed: {}", e),
    }
    match counter.property_value("count").get::<u32>() {
        Ok(val) => println!("count as u32: {}", val),
        Err(e) => println!("count as u32 failed: {}", e),
    }
}

#[test]
fn debug_dynbitrate_properties() {
    init_for_tests();

    let dynbitrate = create_dynbitrate();

    // Let's inspect what properties are actually available and their types
    let klass = dynbitrate.element_class();
    let properties = klass.list_properties();

    for prop in properties {
        println!(
            "DynBitrate Property '{}': Type={:?}, Value type={:?}",
            prop.name(),
            prop.type_(),
            prop.value_type()
        );
    }

    // Try to get the target-loss-pct property and see what type it actually returns
    match dynbitrate.property_value("target-loss-pct").type_() {
        t => println!("target-loss-pct property actual GType: {:?}", t),
    }

    // Try to get it as different types
    match dynbitrate.property_value("target-loss-pct").get::<f64>() {
        Ok(val) => println!("target-loss-pct as f64: {}", val),
        Err(e) => println!("target-loss-pct as f64 failed: {}", e),
    }
    match dynbitrate.property_value("target-loss-pct").get::<u64>() {
        Ok(val) => println!("target-loss-pct as u64: {}", val),
        Err(e) => println!("target-loss-pct as u64 failed: {}", e),
    }
}
