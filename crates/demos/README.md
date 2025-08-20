# demos

Demo programs showcasing network simulation backends and RIST testing capabilities.

## Overview

This crate contains demonstration programs that showcase the capabilities of the RIST bonding testbench ecosystem. These demos serve as both examples for users learning the system and validation tools for developers working on the various backend implementations.

## Features

- **Backend Demonstrations**: Examples using the netns-testbench backend
- **Scenario Showcases**: Demonstrations of various network conditions and presets
- **RIST Protocol Examples**: Real-world RIST bonding and failover scenarios
- **Observability Integration**: Examples of metrics collection and monitoring
- **Performance Comparisons**: Side-by-side backend performance comparisons

## Available Demos

### Network Simulation Demos

#### `test_network_sim_demo`
Demonstrates the netns-testbench backend capabilities.

```bash
cargo run --bin test_network_sim_demo
```

**Features:**
- Basic network impairment simulation
- Simple scenario execution
- Performance measurements
- Legacy backend compatibility

#### `test_netns_demo`
Showcases the advanced netns-testbench backend.

```bash
# Requires sudo for network namespace operations
sudo cargo run --bin test_netns_demo
```

**Features:**
- Network namespace isolation
- Realistic traffic control
- Multi-link topology setup
- Advanced impairment modeling

### Scenario Demonstrations

#### `enhanced_scheduler`
Demonstrates dynamic network condition scheduling.

```bash
sudo cargo run --bin enhanced_scheduler
```

**Features:**
- Time-based network condition changes
- Complex scheduling scenarios
- Real-time parameter updates
- Performance impact analysis

#### `test_race_car_demo`
High-performance networking demonstration simulating racing telemetry.

```bash
cargo run --bin test_race_car_demo
```

**Features:**
- High-throughput data streaming
- Low-latency requirements
- Multi-link bonding
- Real-time performance monitoring

### Observability Demos

#### `observability_demo`
Comprehensive monitoring and metrics collection demonstration.

```bash
cargo run --bin observability_demo --features observability-demo
```

**Features:**
- Prometheus metrics export
- CSV data recording
- Real-time dashboard setup
- Performance analysis tools

## Usage Examples

### Basic Network Simulation

```rust
// From test_network_sim_demo.rs
use scenarios::presets::Presets;
use netlink_sim::NetworkSimulator;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a 4G mobile scenario
    let scenario = Presets::mobile_4g("demo");
    
    // Setup simulator
    let mut simulator = NetworkSimulator::new();
    simulator.apply_scenario(&scenario).await?;
    
    // Run demo traffic
    simulator.run_demo_traffic(Duration::from_secs(60)).await?;
    
    // Display results
    let stats = simulator.get_statistics();
    println!("Average throughput: {} bps", stats.average_throughput);
    println!("Packet loss: {:.2}%", stats.packet_loss_percent);
    
    Ok(())
}
```

### Advanced Network Namespace Testing

```rust
// From test_netns_demo.rs
use netns_testbench::NetNsTestbench;
use scenarios::ScenarioBuilder;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create custom scenario with degrading conditions
    let scenario = ScenarioBuilder::new("degradation_test")
        .add_link("primary", |link| {
            link.bandwidth_mbps(10.0)
                .latency_ms(20.0)
                .schedule(Schedule::new()
                    .at_time(30.0, |spec| spec.packet_loss_percent(1.0))
                    .at_time(60.0, |spec| spec.packet_loss_percent(5.0))
                )
        })
        .build();
    
    // Setup testbench
    let mut testbench = NetNsTestbench::new(Default::default()).await?;
    testbench.apply_scenario(&scenario).await?;
    
    // Run RIST bonding test
    let results = testbench.run_rist_bonding_test(
        "rist://192.168.100.2:5004",
        Duration::from_secs(120)
    ).await?;
    
    // Display comprehensive results
    println!("RIST Bonding Demo Results:");
    println!("========================");
    for (time, stats) in results.time_series {
        println!("{}s: {} bps, {:.1}% loss", 
                time, stats.throughput, stats.packet_loss);
    }
    
    Ok(())
}
```

### Performance Comparison Demo

```rust
// Compare backends side-by-side
use demos::performance_comparison;

#[tokio::main]  
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let scenario = Presets::mobile_4g("comparison");
    
    // Test with netns-testbench backend
    let netlink_results = performance_comparison::test_with_netlink_sim(&scenario).await?;
    
    // Test with netns-testbench backend  
    let netns_results = performance_comparison::test_with_netns_testbench(&scenario).await?;
    
    // Compare results
    performance_comparison::generate_comparison_report(
        &netlink_results, 
        &netns_results,
        "comparison_report.html"
    ).await?;
    
    Ok(())
}
```

## Demo Configuration

### Feature Flags
```toml
# Enable specific demo features
[features]
netns-sim = ["netns-testbench", "tracing", "tracing-subscriber"]
netns-sim = ["netns-testbench", "tracing-subscriber"]
observability-demo = ["observability", "tracing", "tracing-subscriber"]
```

### Environment Configuration
```bash
# Configure demo duration
export DEMO_DURATION=120

# Enable detailed logging
export RUST_LOG=demos=info,netns_testbench=debug

# Set output directory for results
export DEMO_OUTPUT_DIR=./demo_results

# Configure Prometheus metrics endpoint
export PROMETHEUS_PORT=9090
```

## Demo Scenarios

### Mobile Network Simulation
Demonstrates RIST performance under various mobile network conditions:
- 4G/5G network characteristics
- Handover scenarios
- Variable bandwidth conditions
- High-latency satellite backup links

### Data Center Networking
Shows performance in data center environments:
- Low-latency, high-bandwidth links
- Network congestion scenarios
- Multi-path routing
- Load balancing demonstrations

### Internet Backbone Simulation
Demonstrates wide-area network scenarios:
- Continental connectivity
- Submarine cable characteristics
- BGP route changes
- CDN edge scenarios

### Extreme Conditions
Tests system resilience under challenging conditions:
- Very high packet loss (>10%)
- Extreme latency (>1000ms)
- Bandwidth constraints (<100 Kbps)
- Network partitions and healing

## Integration with Other Crates

### Scenarios Integration
```rust
use scenarios::{ScenarioBuilder, presets::Presets};

// Use predefined scenarios
let scenario = Presets::satellite_link("demo");

// Or build custom scenarios
let custom = ScenarioBuilder::new("custom_demo")
    .add_link("wan", |link| {
        link.latency_ms(200.0)
            .bandwidth_kbps(1000)
            .packet_loss_percent(3.0)
    })
    .build();
```

### Observability Integration
```rust
use observability::{MetricsCollector, CsvExporter};

// Collect metrics during demo
let mut collector = MetricsCollector::new();
// ... run demo ...

// Export results
let exporter = CsvExporter::new("demo_metrics.csv");
exporter.export(&collector)?;
```

### RIST Elements Integration
```rust
use rist_elements::testing::RistTestPipeline;

// Setup RIST pipeline for demo
let pipeline = RistTestPipeline::builder()
    .source_uri("videotestsrc")
    .sink_uri("rist://192.168.1.100:5004")
    .enable_bonding(true)
    .build()?;

pipeline.run_demo(Duration::from_secs(60)).await?;
```

## Running Demos

### Prerequisites
```bash
# For netns-testbench demos (requires Linux with unshare capabilities)
cargo run --bin test_network_sim_demo

# For netns-testbench demos (requires sudo)
sudo cargo run --bin test_netns_demo

# For observability demos
cargo run --bin observability_demo --features observability-demo
```

### Output Formats
- **Console**: Live progress and summary statistics
- **CSV**: Detailed time-series metrics data
- **JSON**: Structured results for analysis
- **HTML**: Visual reports with charts and graphs

### Automated Demo Execution
```bash
# Run all demos with automated reporting
./run_demo_suite.sh

# Run specific demo category
cargo run --bin demo_runner -- --category network-namespace

# Run with custom scenarios
cargo run --bin demo_runner -- --config ./custom_demo_config.json
```

## Educational Use

These demos are designed for:
- **Learning**: Understanding RIST bonding concepts
- **Validation**: Verifying system behavior under various conditions
- **Benchmarking**: Comparing performance across different configurations
- **Development**: Testing new features and scenarios
- **Demonstration**: Showing capabilities to stakeholders

Each demo includes detailed comments explaining the networking concepts, RIST protocol behavior, and measurement techniques being demonstrated.