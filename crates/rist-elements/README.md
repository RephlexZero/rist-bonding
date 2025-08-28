# rist-elements

Advanced GStreamer elements for RIST (Reliable Internet Stream Transport) multi-path bonding and adaptive streaming control.

## Overview

This crate implements sophisticated GStreamer elements specifically designed for RIST protocol applications with multi-path bonding capabilities. The core elements enable intelligent traffic distribution, automatic load balancing, and adaptive bitrate control for resilient video streaming over multiple network paths.

**Primary Use Cases:**
- Live video streaming with network redundancy
- Multi-path RIST bonding for increased reliability
- Adaptive bitrate streaming based on network conditions
- Production-grade streaming with automatic failover

## Key Features

- **Advanced RIST Dispatcher**: Intelligent multi-path RTP distribution using Smooth Weighted Round Robin
- **Dynamic Load Balancing**: Real-time traffic distribution based on RIST statistics
- **Automatic Failover**: Link failure detection and seamless traffic redistribution
- **Adaptive Bitrate Control**: Statistics-driven encoder bitrate management
- **Performance Monitoring**: Comprehensive metrics collection and reporting
- **Native GStreamer Integration**: Full GStreamer element implementation with proper event handling
- **Comprehensive Testing**: Extensive test suite with mock elements and integration tests

## Quick Start

### Installation and Plugin Registration

```bash
# Build the plugin
cargo build --release --all-features

# Install to GStreamer plugin directory (system-wide)
sudo cp target/release/libgstristelements.so /usr/lib/gstreamer-1.0/

# Or test locally with plugin path
export GST_PLUGIN_PATH=$PWD/target/release

# Verify installation
gst-inspect-1.0 ristdispatcher
gst-inspect-1.0 dynbitrate
```

### Basic Multi-Path RIST Pipeline

```bash
# Simple dual-path bonding
gst-launch-1.0 \
  videotestsrc pattern=ball ! \
  x264enc bitrate=2000 tune=zerolatency ! \
  rtph264pay ! \
  ristdispatcher name=d \
  d.src_0 ! ristsink uri=rist://primary.example.com:1968 \
  d.src_1 ! ristsink uri=rist://backup.example.com:1968
```

## Key Elements

### RistDispatcher (`ristdispatcher`)

A sophisticated multi-output RTP dispatcher that implements RIST bonding concepts:

**Features:**
- Multiple source pad support with request pad creation
- Configurable per-pad weights for load balancing  
- Dynamic weight adjustment based on link performance
- Buffer distribution across multiple network paths
- Event forwarding (EOS, flush, segment, caps)
- Statistics collection and reporting

**Properties:**
- `weights`: Array of floating-point weights for each output pad
- `enable-stats`: Enable detailed statistics collection
- `failover-timeout`: Link failure detection timeout (ms)
- `rebalance-interval`: Dynamic rebalancing update interval (ms)

**Pads:**
- Sink pad: Receives RTP stream input
- Source pads: Request pads for multiple output streams (`src_%u`)

### Usage in GStreamer Pipeline

```bash
# Basic multi-path distribution
gst-launch-1.0 videotestsrc ! rtpvrawpay ! ristdispatcher name=d \
  d.src_0 ! udpsink host=192.168.1.100 port=5004 \
  d.src_1 ! udpsink host=192.168.1.101 port=5004

# With custom weights (70% / 30% distribution)
gst-launch-1.0 videotestsrc ! rtpvrawpay ! \
  ristdispatcher weights="0.7,0.3" name=d \
  d.src_0 ! udpsink host=192.168.1.100 port=5004 \
  d.src_1 ! udpsink host=192.168.1.101 port=5004

# With statistics enabled
gst-launch-1.0 videotestsrc ! rtpvrawpay ! \
  ristdispatcher enable-stats=true name=d \
  d.src_0 ! udpsink host=primary.example.com port=5004 \
  d.src_1 ! udpsink host=backup.example.com port=5004
```

## Rust API Usage

### Creating and Configuring Elements

```rust
use gstreamer as gst;
use gstreamer::prelude::*;

// Initialize GStreamer
gst::init().unwrap();

// Create dispatcher element
let dispatcher = gst::ElementFactory::make("ristdispatcher")
    .property("weights", &vec![0.6f32, 0.4f32])
    .property("enable-stats", true)
    .build()
    .expect("Failed to create ristdispatcher");

// Create pipeline
let pipeline = gst::Pipeline::new();
let source = gst::ElementFactory::make("videotestsrc")
    .property("num-buffers", 1000)
    .build()
    .unwrap();

let rtp_pay = gst::ElementFactory::make("rtpvrawpay").build().unwrap();

// Add elements to pipeline
pipeline.add_many(&[&source, &rtp_pay, &dispatcher]).unwrap();
gst::Element::link_many(&[&source, &rtp_pay, &dispatcher]).unwrap();

// Request output pads and link to sinks
let src_pad_0 = dispatcher.request_pad_simple("src_%u").unwrap();
let src_pad_1 = dispatcher.request_pad_simple("src_%u").unwrap();

let sink1 = gst::ElementFactory::make("udpsink")
    .property("host", "192.168.1.100")
    .property("port", 5004i32)
    .build()
    .unwrap();

let sink2 = gst::ElementFactory::make("udpsink")
    .property("host", "192.168.1.101") 
    .property("port", 5004i32)
    .build()
    .unwrap();

pipeline.add_many(&[&sink1, &sink2]).unwrap();

src_pad_0.link(&sink1.static_pad("sink").unwrap()).unwrap();
src_pad_1.link(&sink2.static_pad("sink").unwrap()).unwrap();
```

### Dynamic Weight Adjustment

```rust
use std::time::Duration;
use tokio::time::interval;

// Create interval for periodic weight updates
let mut update_timer = interval(Duration::from_secs(5));

loop {
    update_timer.tick().await;
    
    // Get current link performance (from external monitoring)
    let link1_quality = get_link_quality("192.168.1.100").await;
    let link2_quality = get_link_quality("192.168.1.101").await;
    
    // Calculate new weights based on link quality
    let total_quality = link1_quality + link2_quality;
    let weight1 = link1_quality / total_quality;
    let weight2 = link2_quality / total_quality;
    
    // Update dispatcher weights
    dispatcher.set_property("weights", &vec![weight1, weight2]);
    
    println!("Updated weights: {:.2}, {:.2}", weight1, weight2);
}
```

### Statistics Collection

```rust
use glib::prelude::*;

// Enable statistics collection
dispatcher.set_property("enable-stats", true);

// Get statistics periodically
let stats_timer = interval(Duration::from_secs(10));
loop {
    stats_timer.tick().await;
    
    // Get per-pad statistics
    let pad_count: u32 = dispatcher.property("pad-count");
    
    for pad_idx in 0..pad_count {
        let bytes_sent: u64 = dispatcher.property(&format!("pad-{}-bytes", pad_idx));
        let packets_sent: u64 = dispatcher.property(&format!("pad-{}-packets", pad_idx));
        let last_activity: u64 = dispatcher.property(&format!("pad-{}-last-activity", pad_idx));
        
        println!("Pad {}: {} bytes, {} packets, last activity: {}ms ago", 
                 pad_idx, bytes_sent, packets_sent, last_activity);
    }
}
```

## Testing Support

### Test Plugin Features

When built with the `test-plugin` feature, additional testing utilities are available:

```rust
use rist_elements::testing::{TestCounterSink, TestingUtils};

// Create mock elements for testing
let counter_sink = TestCounterSink::new();
counter_sink.set_property("count-eos", true);
counter_sink.set_property("count-flush", true);

// Run test scenarios
TestingUtils::test_eos_propagation(&dispatcher, &counter_sink);
TestingUtils::test_flush_handling(&dispatcher, &counter_sink);
```

### Integration Testing

```rust
use integration_tests::element_pad_semantics::DispatcherTestingProvider;

// Implement testing provider for rist-elements
struct RistElementsTestProvider;

impl DispatcherTestingProvider for RistElementsTestProvider {
    fn create_dispatcher(weights: Option<&[f32]>) -> gst::Element {
        let weights = weights.unwrap_or(&[1.0]);
        gst::ElementFactory::make("ristdispatcher")
            .property("weights", weights)
            .build()
            .expect("Failed to create ristdispatcher")
    }
    
    fn init_for_tests() {
        gst::init().unwrap();
    }
    
    // ... implement other required methods
}

// Run comprehensive element tests
#[test]
fn test_rist_dispatcher_behavior() {
    integration_tests::element_pad_semantics::test_caps_negotiation_and_proxying::<RistElementsTestProvider>();
    integration_tests::element_pad_semantics::test_eos_event_fanout::<RistElementsTestProvider>();
    integration_tests::element_pad_semantics::test_flush_event_handling::<RistElementsTestProvider>();
}
```

## Building and Installation

### Build Requirements

```bash
# Install GStreamer development packages (Ubuntu/Debian)
sudo apt-get install libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev

# Install GStreamer development packages (Fedora/RHEL)
sudo dnf install gstreamer1-devel gstreamer1-plugins-base-devel

# Build the plugin
cargo build --release

# Install plugin to GStreamer plugin directory
sudo cp target/release/libgstristelements.so /usr/lib/gstreamer-1.0/
```

### Plugin Registration

```bash
# Verify plugin installation
gst-inspect-1.0 ristdispatcher

# Test plugin loading
GST_PLUGIN_PATH=target/release gst-inspect-1.0 ristdispatcher
```

## Testing

The crate includes comprehensive integration and performance tests:

```bash
# Run all tests
cargo test

# Run tests with clean output (suppresses GStreamer debug hex dumps)
./scripts/test-clean.sh

# Run specific test categories
cargo test integration
cargo test performance_benchmarks
cargo test thread_safety

# Run individual test with output
cargo test test_memory_usage_under_load -- --nocapture
```

### Suppressing GStreamer Debug Output

When running tests, you may see large hex dumps from GStreamer's internal buffer debugging. To suppress these:

```bash
# Use environment variables
export GST_DEBUG=0
export GST_DEBUG_DUMP_DOT_DIR=""
export GST_DEBUG_NO_COLOR=1
cargo test

# Or use the provided clean test script
./scripts/test-clean.sh integration
```

The hex dumps are harmless but can make test output verbose. They typically appear during high-load performance tests or when multiple tests run concurrently.

## Advanced Features

### Network-Aware Load Balancing

Integration with network simulation backends for intelligent traffic distribution:

```rust
use netlink_sim::NetworkMonitor;

// Monitor network conditions
let mut monitor = NetworkMonitor::new();
monitor.add_link("192.168.1.100", "primary");
monitor.add_link("192.168.1.101", "backup");

// Adjust weights based on real-time conditions
loop {
    let conditions = monitor.get_current_conditions().await;
    
    let weights = conditions.iter()
        .map(|link| 1.0 / (1.0 + link.latency_ms / 100.0 + link.packet_loss * 10.0))
        .collect::<Vec<f32>>();
    
    dispatcher.set_property("weights", &weights);
    
    tokio::time::sleep(Duration::from_secs(1)).await;
}
```

### Performance Monitoring

The RIST elements emit performance metrics via GStreamer bus messages that can be monitored:
bus_collector.add_element_filter("ristdispatcher", |msg| {
    match msg.view() {
        gst::MessageView::Element(element_msg) => {
            if let Some(structure) = element_msg.structure() {
                if structure.name() == "rist-dispatcher-stats" {
                    // Extract and record statistics
                    metrics_collector.record_element_stats(structure);
                }
            }
        },
        _ => {}
    }
});
```

## Configuration

### Element Properties

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `weights` | `Vec<f32>` | `[1.0]` | Per-pad traffic distribution weights |
| `enable-stats` | `bool` | `false` | Enable detailed statistics collection |
| `failover-timeout` | `u32` | `5000` | Link failure detection timeout (ms) |
| `rebalance-interval` | `u32` | `1000` | Dynamic rebalancing update interval (ms) |
| `pad-count` | `u32` | `0` | Number of active output pads (read-only) |

### Statistics Properties (when `enable-stats=true`)

| Property Pattern | Type | Description |
|------------------|------|-------------|
| `pad-{n}-bytes` | `u64` | Total bytes sent through pad n |
| `pad-{n}-packets` | `u64` | Total packets sent through pad n |
| `pad-{n}-last-activity` | `u64` | Time since last packet (ms) |
| `pad-{n}-weight` | `f32` | Current weight for pad n |

## Performance Considerations

- **Memory Usage**: Minimal buffering with zero-copy where possible
- **CPU Usage**: Efficient packet distribution with configurable threading
- **Latency**: Low-latency packet forwarding suitable for real-time applications
- **Throughput**: Tested with high-bitrate video streams (>100 Mbps)

## Compatibility

- **GStreamer**: Supports GStreamer 1.14+ (tested with 1.20+)
- **RTP**: Compatible with standard RTP payloaders
- **Network**: Works with any GStreamer network sink elements
- **Platforms**: Linux, macOS, Windows (with appropriate GStreamer installation)

## Project Integration

This crate is part of the [RIST Bonding](../../README.md) project. For complete documentation:

- **[Main Project Documentation](../../README.md)**: Overview, architecture, and pipeline examples
- **[Testing Guide](../../docs/testing/README.md)**: Comprehensive testing setup and troubleshooting
- **[Plugin Documentation](../../docs/plugins/README.md)**: Detailed element configuration and usage
- **[Network Simulation](../network-sim/README.md)**: Network condition simulation for testing
- **[Development Environment](../../docs/testing/DOCKER_TESTING.md)**: Container-based development