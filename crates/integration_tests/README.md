# integration_tests

Shared integration testing utilities and test suites for RIST network simulation.

## Overview

This crate provides comprehensive integration testing capabilities for RIST bonding and network simulation components. It includes shared test utilities, element pad semantics tests, and automated integration test suites that work across different backends and GStreamer elements.

## Features

- **Element Pad Semantics**: Generic tests for GStreamer element behavior
- **Cross-Backend Testing**: Tests that work with the netns-testbench backend
- **RIST-Specific Tests**: Bonding, failover, and retransmission testing
- **Network Condition Testing**: Automated testing under various network impairments
- **Performance Validation**: Throughput, latency, and reliability testing
- **Event Handling**: EOS, flush, and sticky event testing

## Key Components

### Shared Testing Traits
- `DispatcherTestingProvider`: Common interface for testing different RIST dispatcher implementations
- Generic test functions that can be reused across different element implementations

### Test Categories
- **Element Behavior**: Caps negotiation, event handling, pad lifecycle
- **Network Performance**: Throughput, latency, packet loss resilience
- **RIST Protocol**: Bonding behavior, retransmission, failover scenarios
- **Integration**: End-to-end testing with real network conditions

## Usage

### Implementing Tests for Custom Elements

```rust
use integration_tests::element_pad_semantics::DispatcherTestingProvider;

struct MyRistDispatcherTestProvider;

impl DispatcherTestingProvider for MyRistDispatcherTestProvider {
    fn create_dispatcher(weights: Option<&[f32]>) -> gst::Element {
        // Create your dispatcher element
        gst::ElementFactory::make("myristdispatcher")
            .property("weights", weights.unwrap_or(&[1.0]))
            .build()
            .expect("Failed to create dispatcher")
    }
    
    fn create_fake_sink() -> gst::Element {
        gst::ElementFactory::make("fakesink")
            .property("sync", false)
            .build()
            .expect("Failed to create fake sink")
    }
    
    // Implement other required methods...
    
    fn init_for_tests() {
        gst::init().unwrap();
    }
}

#[test]
fn test_my_dispatcher_caps_negotiation() {
    integration_tests::element_pad_semantics::test_caps_negotiation_and_proxying::<MyRistDispatcherTestProvider>();
}
```

### Running Element Behavior Tests

```rust
use integration_tests::element_pad_semantics::*;

// Test EOS event fanout across multiple outputs
test_eos_event_fanout::<MyRistDispatcherTestProvider>();

// Test flush event handling
test_flush_event_handling::<MyRistDispatcherTestProvider>();

// Test sticky events replay for new pads
test_sticky_events_replay::<MyRistDispatcherTestProvider>();

// Test pad removal and cleanup
test_pad_removal_and_cleanup::<MyRistDispatcherTestProvider>();
```

## Available Generic Tests

### Element Pad Semantics
- **Caps Negotiation**: Verifies proper caps negotiation between elements
- **EOS Event Fanout**: Tests End-of-Stream propagation to all output pads
- **Flush Event Handling**: Validates flush start/stop event handling
- **Sticky Events Replay**: Ensures new pads receive necessary sticky events
- **Pad Removal**: Tests proper cleanup when removing request pads

### Network Performance Tests
- **Throughput Under Load**: Measures performance under high data rates
- **Latency Resilience**: Tests behavior with high latency links
- **Packet Loss Recovery**: Validates RIST retransmission mechanisms
- **Link Failover**: Tests bonding behavior during link failures
- **Congestion Handling**: Behavior under network congestion conditions

### RIST Protocol Tests
- **Multi-Link Bonding**: Tests traffic distribution across multiple links
- **Retransmission Logic**: Validates RIST retransmission behavior
- **Buffer Management**: Tests buffer sizing and management
- **Statistics Reporting**: Validates performance metrics reporting

## Integration with Other Crates

### With `scenarios`
```rust
use scenarios::presets::Presets;
use integration_tests::network_tests;

#[tokio::test]
async fn test_rist_under_mobile_4g() {
    let scenario = Presets::mobile_4g("integration_test");
    network_tests::test_scenario_performance(&scenario).await.unwrap();
}
```

### With `observability`
```rust
use observability::MetricsCollector;
use integration_tests::performance_tests;

#[test]
fn test_with_metrics_collection() {
    let mut collector = MetricsCollector::new();
    performance_tests::test_throughput_with_metrics(&mut collector);
    
    // Validate collected metrics
    assert!(collector.get_average_throughput() > 500_000); // 500 Kbps minimum
}
```

## Running Tests

### Unit Tests
```bash
# Run all integration tests
cargo test -p integration_tests

# Run specific test category
cargo test -p integration_tests element_pad_semantics

# Run with detailed output
cargo test -p integration_tests -- --nocapture
```

### With Different Backends
```bash
# Test with netns-testbench backend
cargo test -p integration_tests --features netns-backend

# Test with netns-testbench backend  
cargo test -p integration_tests --features netlink-backend
```

### Performance Tests
```bash
# Run performance validation tests
cargo test -p integration_tests performance --release

# Run stress tests (requires sudo for netns-testbench)
sudo cargo test -p integration_tests stress_tests
```

## Test Configuration

Tests can be configured via environment variables:

```bash
# Set test duration
export INTEGRATION_TEST_DURATION=60

# Enable verbose logging
export RUST_LOG=integration_tests=debug

# Configure test backend
export INTEGRATION_TEST_BACKEND=netns-testbench

# Set metrics collection
export INTEGRATION_TEST_METRICS=true
```

## CI/CD Integration

The integration tests are designed for continuous integration:

```yaml
# .github/workflows/integration_tests.yml
- name: Run Integration Tests
  run: |
    cargo test -p integration_tests --verbose
    
- name: Run Performance Tests
  run: |
    cargo test -p integration_tests performance --release
    
- name: Upload Test Results
  uses: actions/upload-artifact@v2
  with:
    name: integration-test-results
    path: target/integration_tests/
```

## Test Categories

### Functional Tests
- Element creation and initialization
- Property setting and getting  
- State transitions
- Event handling
- Pad linking and unlinking

### Performance Tests  
- Throughput measurements
- Latency characterization
- Memory usage validation
- CPU usage profiling
- Resource leak detection

### Stress Tests
- High data rate handling
- Long-duration stability
- Rapid state changes
- Resource exhaustion scenarios
- Error recovery testing

### Integration Tests
- Full pipeline testing
- Backend compatibility
- Cross-element communication
- Real network condition simulation
- End-to-end RIST protocol validation