# scenarios

Network scenario definitions and presets for RIST testbench simulations.

## Overview

This crate provides data models for network impairments and realistic 4G/5G behavior presets that can be used by the `netns-testbench` backend. It defines comprehensive network conditions including latency, packet loss, jitter, and bandwidth constraints.

## Features

- **Network Impairment Models**: Define realistic network conditions with latency, jitter, packet loss, and bandwidth constraints
- **4G/5G Presets**: Pre-configured scenarios for common mobile network conditions
- **Scenario Builder**: Fluent API for constructing complex network scenarios
- **Schedule Support**: Time-based network condition changes for dynamic testing
- **Bidirectional Links**: Independent uplink/downlink configuration

## Key Components

- `TestScenario`: Main scenario definition with network links and scheduling
- `LinkSpec`: Individual network link configuration with impairments
- `DirectionSpec`: Separate uplink/downlink parameters
- `ScenarioBuilder`: Fluent builder pattern for scenario construction
- `Presets`: Pre-defined scenarios for common network conditions
- `Schedule`: Time-based network condition changes

## Usage

### Basic Scenario Creation

```rust
use scenarios::{ScenarioBuilder, presets::Presets};

// Create a basic scenario with latency and packet loss
let scenario = ScenarioBuilder::new("test_scenario")
    .add_link("link1", |link| {
        link.latency_ms(50.0)
            .packet_loss_percent(2.0)
            .bandwidth_kbps(Some(1000))
    })
    .build();

// Use a preset for 4G conditions
let mobile_scenario = Presets::mobile_4g("4g_test");
```

### Dynamic Scenarios with Scheduling

```rust
use scenarios::{ScenarioBuilder, Schedule};

let dynamic_scenario = ScenarioBuilder::new("dynamic_test")
    .add_link("link1", |link| {
        link.latency_ms(20.0)
            .schedule(Schedule::new()
                .at_time(30.0, |spec| spec.latency_ms(100.0))
                .at_time(60.0, |spec| spec.packet_loss_percent(5.0))
            )
    })
    .build();
```

## Available Presets

- `perfect_link()`: Zero impairments for baseline testing
- `fiber_link()`: Low-latency, high-reliability fiber connection
- `mobile_4g()`: Typical 4G mobile network conditions
- `mobile_5g()`: 5G network with improved characteristics
- `satellite_link()`: High-latency satellite connection
- `congested_wifi()`: Overloaded WiFi with variable conditions

## Integration

This crate is designed to work seamlessly with:
- `netns-testbench`: Linux network namespace testing
- `netns-testbench`: Network namespace simulation backend

The scenarios defined here can be executed by either backend while maintaining consistent behavior and measurement capabilities.