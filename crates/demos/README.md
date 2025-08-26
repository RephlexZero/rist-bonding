# demos# demos



Demo programs showcasing network simulation and RIST testing capabilities.Demo programs showcasing network simulation backends and RIST testing capabilities.



## Overview## Overview



This crate contains demonstration programs that showcase how to use the RIST bonding testbench ecosystem. These demos serve as examples for users learning the system and provide working demonstrations of the `scenarios` and `netns-testbench` integration.This crate contains demonstration programs that showcase the capabilities of the RIST bonding testbench ecosystem. These demos serve as both examples for users learning the system and validation tools for developers working on the various backend implementations.



## Available Demos## Features



### `test_netns_demo`- **Backend Demonstrations**: Examples using the netns-testbench backend

Demonstrates the netns-testbench backend with network namespace isolation.- **Scenario Showcases**: Demonstrations of various network conditions and presets

- **RIST Protocol Examples**: Real-world RIST bonding and failover scenarios

```bash- **Network Scenario Simulation**: Pre-configured scenarios for common RIST use cases

# Requires sudo for network namespace operations- **Real-time Performance Analysis**: Live monitoring of network conditions and adaptation

sudo cargo run --bin test_netns_demo- **Performance Comparisons**: Side-by-side backend performance comparisons

```

## Available Demos

**Features:**

- Network namespace isolation### Network Simulation Demos

- Realistic traffic control using Linux TC

- Multi-link topology setup#### `test_network_sim_demo`

- Network impairment modeling (latency, loss, bandwidth limits)Demonstrates the netns-testbench backend capabilities.

- Integration with scenarios crate

```bash

### `enhanced_scheduler`cargo run --bin test_network_sim_demo

Demonstrates dynamic network condition scheduling and parameter updates.```



```bash**Features:**

# Requires sudo for network namespace operations- Basic network impairment simulation

sudo cargo run --bin enhanced_scheduler- Simple scenario execution

```- Performance measurements

- Legacy backend compatibility

**Features:**

- Time-based network condition changes#### `test_netns_demo`

- Complex scheduling scenariosShowcases the advanced netns-testbench backend.

- Real-time parameter updates during simulation

- Performance impact analysis```bash

# Requires sudo for network namespace operations

## Usage Examplessudo cargo run --bin test_netns_demo

```

### Basic Network Namespace Testing

**Features:**

The demos show how to integrate the scenarios and netns-testbench crates:- Network namespace isolation

- Realistic traffic control

```rust- Multi-link topology setup

use scenarios::presets::Presets;- Advanced impairment modeling

use netns_testbench::NetworkOrchestrator;

### Scenario Demonstrations

#[tokio::main]

async fn main() -> Result<(), Box<dyn std::error::Error>> {#### `enhanced_scheduler`

    // Use a predefined scenarioDemonstrates dynamic network condition scheduling.

    let scenario = Presets::mobile_4g("demo");

    ```bash

    // Create orchestrator with network namespace backendsudo cargo run --bin enhanced_scheduler

    let mut orchestrator = NetworkOrchestrator::new(5004).await?;```

    

    // Apply the scenario**Features:**

    orchestrator.apply_scenario(&scenario).await?;- Time-based network condition changes

    - Complex scheduling scenarios

    // Run for duration specified in scenario- Real-time parameter updates

    tokio::time::sleep(std::time::Duration::from_secs(60)).await;- Performance impact analysis

    

    Ok(())#### `test_race_car_demo`

}High-performance networking demonstration simulating racing telemetry.

```

```bash

### Custom Scenario Buildingcargo run --bin test_race_car_demo

```

```rust

use scenarios::{ScenarioBuilder, Schedule};**Features:**

- High-throughput data streaming

let scenario = ScenarioBuilder::new("custom_demo")- Low-latency requirements

    .description("Custom network conditions")- Multi-link bonding

    .duration_seconds(120)- Real-time performance monitoring

    .add_link("primary", |link| {

        link.bandwidth_mbps(10.0)## Usage Examples

            .latency_ms(20.0)

            .packet_loss_percent(0.5)### Basic Network Simulation

    })

    .add_link("backup", |link| {```rust

        link.bandwidth_mbps(5.0)// From test_network_sim_demo.rs

            .latency_ms(100.0) use scenarios::presets::Presets;

            .packet_loss_percent(1.0)use netlink_sim::NetworkSimulator;

    })

    .build();#[tokio::main]

```async fn main() -> Result<(), Box<dyn std::error::Error>> {

    // Create a 4G mobile scenario

## Prerequisites    let scenario = Presets::mobile_4g("demo");

    

### Linux Requirements    // Setup simulator

- Linux system with network namespace support    let mut simulator = NetworkSimulator::new();

- `sudo` privileges for network namespace operations    simulator.apply_scenario(&scenario).await?;

- Traffic control (`tc`) utilities installed    

    // Run demo traffic

### Build Requirements    simulator.run_demo_traffic(Duration::from_secs(60)).await?;

```bash    

# Install if not present    // Display results

sudo apt-get install iproute2 iptables    let stats = simulator.get_statistics();

    println!("Average throughput: {} bps", stats.average_throughput);

# Build the demos    println!("Packet loss: {:.2}%", stats.packet_loss_percent);

cargo build --bin test_netns_demo    

cargo build --bin enhanced_scheduler    Ok(())

```}

```

## Running the Demos

### Advanced Network Namespace Testing

### Environment Setup

```bash```rust

# Enable detailed logging// From test_netns_demo.rs

export RUST_LOG=demos=info,netns_testbench=debug,scenarios=infouse netns_testbench::NetNsTestbench;

use scenarios::ScenarioBuilder;

# Set output directory (optional)

export DEMO_OUTPUT_DIR=./demo_results#[tokio::main]

```async fn main() -> Result<(), Box<dyn std::error::Error>> {

    // Create custom scenario with degrading conditions

### Execution    let scenario = ScenarioBuilder::new("degradation_test")

```bash        .add_link("primary", |link| {

# Run the basic network namespace demo            link.bandwidth_mbps(10.0)

sudo cargo run --bin test_netns_demo                .latency_ms(20.0)

                .schedule(Schedule::new()

# Run the enhanced scheduler demo                    .at_time(30.0, |spec| spec.packet_loss_percent(1.0))

sudo cargo run --bin enhanced_scheduler                    .at_time(60.0, |spec| spec.packet_loss_percent(5.0))

                )

# Run with verbose logging        })

sudo RUST_LOG=debug cargo run --bin test_netns_demo        .build();

```    

    // Setup testbench

## Integration with Other Crates    let mut testbench = NetNsTestbench::new(Default::default()).await?;

    testbench.apply_scenario(&scenario).await?;

### Scenarios Integration    

The demos extensively use the `scenarios` crate:    // Run RIST bonding test

- Predefined scenarios via `Presets`    let results = testbench.run_rist_bonding_test(

- Custom scenario building with `ScenarioBuilder`        "rist://192.168.100.2:5004",

- Network condition scheduling        Duration::from_secs(120)

    ).await?;

### Network Testbench Integration      

Direct integration with `netns-testbench`:    // Display comprehensive results

- `NetworkOrchestrator` for network namespace management    println!("RIST Bonding Demo Results:");

- Scenario application and execution    println!("========================");

- Real-time network parameter updates    for (time, stats) in results.time_series {

        println!("{}s: {} bps, {:.1}% loss", 

## Educational Use                time, stats.throughput, stats.packet_loss);

    }

These demos are designed for:    

- **Learning**: Understanding how to integrate scenarios with network simulation    Ok(())

- **Validation**: Testing network conditions and RIST behavior}

- **Development**: Serving as templates for custom testing scenarios```

- **Demonstration**: Showing testbench capabilities

### Performance Comparison Demo

Each demo includes detailed comments explaining the networking concepts, scenario setup, and measurement techniques being demonstrated.

```rust

## Output and Results// Compare backends side-by-side

use demos::performance_comparison;

The demos provide:

- **Console Output**: Live progress and network condition updates#[tokio::main]  

- **Logging**: Detailed operation logs via `tracing`async fn main() -> Result<(), Box<dyn std::error::Error>> {

- **Error Handling**: Comprehensive error reporting and recovery    let scenario = Presets::mobile_4g("comparison");

    

Results include network statistics, timing information, and scenario execution details logged to the console during runtime.    // Test with netns-testbench backend
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
netns-sim = ["netns-testbench", "tracing-subscriber"]
```

### Environment Configuration
```bash
# Configure demo duration
export DEMO_DURATION=120

# Enable detailed logging
export RUST_LOG=demos=info,netns_testbench=debug

# Set output directory for results
export DEMO_OUTPUT_DIR=./demo_results
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