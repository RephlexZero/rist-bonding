# netns-testbench

Linux network namespace testbench for RIST bonding with netlink-based qdisc control.

## Overview

`netns-testbench` is a sophisticated testing framework that uses Linux network namespaces to create realistic network conditions for RIST bonding and network protocol testing. It provides precise control over network impairments using Linux traffic control (tc) and netlink interfaces, enabling high-fidelity simulation of real-world network conditions.

## Features

- **Network Namespace Isolation**: Create isolated network environments for testing
- **Traffic Control Integration**: Use Linux tc queueing disciplines for realistic impairments
- **Netlink-based Control**: Direct kernel interface for precise network parameter control
- **Multi-Link Simulation**: Support for complex multi-path network topologies
- **Real-time Parameter Changes**: Dynamic network condition modification during tests
- **RIST Protocol Testing**: Specialized support for RIST bonding and failover scenarios

## Key Components

### Network Namespace Management
- **Namespace Creation**: Isolated network environments for testing
- **Virtual Interface Setup**: veth pairs and bridge configurations
- **Routing Configuration**: Complex multi-path routing scenarios
- **Process Management**: Execute test processes within namespaces

### Traffic Control
- **Queue Disciplines**: netem, tbf, pfifo, and other qdisc configurations
- **Network Impairments**: Latency, jitter, packet loss, bandwidth limiting
- **Dynamic Updates**: Runtime modification of network parameters
- **Statistics Collection**: Detailed qdisc and interface statistics

### Integration Points
- **Scenarios Integration**: Execute scenarios from the scenarios crate
- **RIST Testing**: Specialized RIST protocol testing capabilities

## Prerequisites

This crate requires Linux with network namespace capabilities and typically needs elevated privileges:

```bash
# Ensure your system supports network namespaces
sudo ip netns list

# Required capabilities
# CAP_NET_ADMIN, CAP_SYS_ADMIN (typically requires sudo)
```

## Usage

### Basic Network Namespace Testing

```rust
use netns_testbench::{NetNsTestbench, TestbenchConfig};
use scenarios::presets::Presets;

#[tokio::test]
async fn test_rist_bonding() {
    let config = TestbenchConfig::default()
        .with_namespace_prefix("rist_test")
        .with_cleanup_on_drop(true);
    
    let mut testbench = NetNsTestbench::new(config).await?;
    
    // Setup network topology
    testbench.create_link_pair("ns1", "ns2").await?;
    testbench.apply_scenario(&Presets::mobile_4g("test")).await?;
    
    // Run RIST test
    let result = testbench.run_rist_test("rist://192.168.1.1:5004").await?;
    
    assert!(result.average_throughput > 500_000); // 500 Kbps
    assert!(result.packet_loss_percent < 1.0);
}
```

### Advanced Multi-Link Configuration

```rust
use netns_testbench::{NetworkTopology, LinkConfig};

let topology = NetworkTopology::builder()
    .add_namespace("sender")
    .add_namespace("receiver")
    .add_link("uplink", LinkConfig::new()
        .source("sender")
        .destination("receiver")
        .latency_ms(50.0)
        .bandwidth_mbps(10.0)
        .packet_loss_percent(1.0)
    )
    .add_link("backup", LinkConfig::new()
        .source("sender") 
        .destination("receiver")
        .latency_ms(200.0)
        .bandwidth_mbps(5.0)
        .packet_loss_percent(0.5)
    )
    .build();

let mut testbench = NetNsTestbench::with_topology(topology).await?;
testbench.start_rist_bonding_test().await?;
```

### Dynamic Network Condition Changes

```rust
use std::time::Duration;
use tokio::time::{sleep, interval};

// Start with good conditions
testbench.apply_impairments("uplink", |config| {
    config.latency_ms(20.0).packet_loss_percent(0.1)
}).await?;

// Gradually degrade the network
let mut degradation_timer = interval(Duration::from_secs(10));
for step in 1..=5 {
    degradation_timer.tick().await;
    
    let loss_percent = 0.1 * step as f64;
    testbench.update_impairments("uplink", |config| {
        config.packet_loss_percent(loss_percent)
    }).await?;
    
    println!("Applied {}% packet loss", loss_percent);
}
```

## Architecture

### Network Namespace Setup
1. **Create Network Namespaces**: Isolated network environments
2. **Setup Virtual Interfaces**: veth pairs connecting namespaces
3. **Configure Routing**: IP addresses and routing tables
4. **Apply Traffic Control**: qdisc configuration for impairments

### Traffic Control Integration
```rust
// Apply netem discipline for network impairments
testbench.apply_qdisc("veth0", QdiscConfig::Netem {
    latency: Duration::from_millis(100),
    jitter: Duration::from_millis(10),
    loss_percent: 2.0,
    duplicate_percent: 0.1,
}).await?;

// Apply bandwidth limiting with token bucket filter
testbench.apply_qdisc("veth0", QdiscConfig::Tbf {
    rate_bps: 1_000_000, // 1 Mbps
    burst_bytes: 32768,
    latency_ms: 50,
}).await?;
```

## Configuration

### TestbenchConfig
```rust
use netns_testbench::TestbenchConfig;

let config = TestbenchConfig::builder()
    .namespace_prefix("rist_test")
    .cleanup_on_drop(true)
    .enable_ip_forwarding(true)
    .base_ip_range("192.168.100.0/24")
    .metrics_collection(true)
    .build();
```

### LinkConfig
```rust
use netns_testbench::LinkConfig;

let link_config = LinkConfig::builder()
    .bandwidth_bps(1_000_000)
    .latency_ms(50.0)
    .jitter_ms(5.0)
    .packet_loss_percent(1.0)
    .corruption_percent(0.01)
    .reorder_percent(0.1)
    .queue_limit_packets(1000)
    .build();
```

## Integration with RIST Elements

### GStreamer Pipeline Testing
```rust
use gstreamer as gst;

// Setup pipeline in network namespace
testbench.execute_in_namespace("sender", || {
    let pipeline = gst::parse_launch(&format!(
        "videotestsrc ! ristenc ! ristsink uri=rist://192.168.100.2:5004"
    ))?;
    
    pipeline.set_state(gst::State::Playing)?;
    
    // Run test...
    Ok(())
}).await?;
```

### Performance Testing
```rust
// Run comprehensive RIST performance test
let test_results = testbench.run_performance_suite(&[
    "mobile_4g", "mobile_5g", "satellite_link", "fiber_link"
]).await?;

for result in test_results {
    println!("Scenario: {}", result.scenario_name);
    println!("Throughput: {} bps", result.average_throughput);
    println!("Latency: {:.2} ms", result.average_latency_ms);
    println!("Loss: {:.3}%", result.packet_loss_percent);
}
```

## Monitoring and Metrics

### Built-in Metrics Collection
```rust
// Enable comprehensive metrics collection
let metrics = testbench.get_metrics_collector();

// Collect interface statistics
let interface_stats = metrics.collect_interface_stats("veth0").await?;

// Collect qdisc statistics  
let qdisc_stats = metrics.collect_qdisc_stats("veth0").await?;
```

## Testing Features

### Scenario Execution
- Execute predefined scenarios from the scenarios crate
- Dynamic scenario modification during test execution
- Scheduled network condition changes
- Automated scenario validation

### RIST-Specific Testing
- Multi-link bonding validation
- Link failover and recovery testing  
- Retransmission behavior analysis
- Buffer management under stress
- Performance characterization

### Advanced Features
- **Container Integration**: Docker/Podman support for isolated testing
- **Multi-Host Testing**: Distributed testing across multiple machines
- **Hardware Simulation**: Simulate specific hardware constraints
- **Protocol Analysis**: Deep packet inspection and protocol analysis

## Safety and Cleanup

The testbench includes comprehensive cleanup mechanisms:

```rust
// Automatic cleanup on drop
impl Drop for NetNsTestbench {
    fn drop(&mut self) {
        // Clean up network namespaces
        // Remove virtual interfaces
        // Reset system network configuration
    }
}

// Manual cleanup for complex scenarios
testbench.cleanup_all().await?;
```

## Performance Considerations

- **Resource Usage**: Network namespaces have minimal overhead
- **Scalability**: Supports dozens of concurrent namespaces
- **Precision**: Kernel-level traffic control for accurate impairments  
- **Real-time**: Suitable for real-time protocol testing
- **Reproducibility**: Deterministic network conditions for consistent testing