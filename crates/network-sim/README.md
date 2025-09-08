# network-sim

A simple yet powerful network simulation library for applying realistic network conditions to Linux network interfaces using Traffic Control (TC). Designed for testing RIST bonding and other network-sensitive applications in controlled environments.

## Overview

This library provides a "set and forget" approach to network simulation, focusing on applying static network parameters rather than complex time-varying scenarios. It integrates seamlessly with Linux's Traffic Control system to create realistic network conditions for testing and development.

**Key Philosophy**: Simple, predictable network parameter application with clean async APIs for modern Rust applications.

## Features

- **Static network parameter application**: Apply fixed delay, packet loss, and rate limiting to network interfaces
- **Predefined network profiles**: Pre-configured "good", "typical", and "poor" network conditions
- **Linux Traffic Control integration**: Uses `tc` to configure kernel qdisc for realistic network behavior
- **Clean async API**: Built with Tokio for non-blocking network operations
- **Structured error handling**: Comprehensive error types for different failure modes
- **Container-friendly**: Designed for use in Docker containers and network namespaces
- **Minimal dependencies**: Focused implementation with essential dependencies only

## Quick Start

```rust
use network_sim::{apply_network_params, NetworkParams, QdiscManager};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a qdisc manager for interface manipulation
    let qdisc_manager = QdiscManager::default();
    
    // Apply typical network conditions (20ms delay, 1% loss, 5Mbps)
    let params = NetworkParams::typical();
    apply_network_params(&qdisc_manager, "veth0", &params).await?;
    
    println!("Applied network simulation to veth0");
    
    // Network interface now has realistic network conditions applied
    // Run your RIST tests, streaming applications, etc.
    
    Ok(())
}
```

## Detailed Usage

### Creating Custom Network Parameters

```rust
use network_sim::NetworkParams;

// Create custom network conditions
let custom_params = NetworkParams {
    delay_ms: 50,         // 50ms one-way delay
    loss_pct: 0.025,      // 2.5% packet loss (0.0..1.0)
    rate_kbps: 3_000,     // 3 Mbps bandwidth limit
    jitter_ms: 5,         // ±5ms jitter
    reorder_pct: 0.0,     // 0%
    duplicate_pct: 0.0,   // 0%
    loss_corr_pct: 0.0,   // 0%
};
```

### Working with Network Namespaces

```rust
use network_sim::{apply_network_params, NetworkParams, QdiscManager};
use tokio::process::Command;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let qdisc_manager = QdiscManager::default();
    
    // Create network namespace for isolated testing
    Command::new("ip")
        .args(&["netns", "add", "test-ns"])
        .status()
        .await?;
    
    // Create veth pair
    Command::new("ip")
        .args(&["link", "add", "veth-test", "type", "veth", "peer", "name", "veth-test-peer"])
        .status()
        .await?;
        
    // Move one end to namespace
    Command::new("ip")
        .args(&["link", "set", "veth-test-peer", "netns", "test-ns"])
        .status()
        .await?;
    
    // Apply network conditions to the interface in namespace
    // Note: You'll need to execute this within the namespace context
    let params = NetworkParams::poor(); // High latency, high loss
    
    // This would need to be executed within the namespace
    // ip netns exec test-ns <apply network params>
    
    Ok(())
}
```

### Integration with RIST testing

```rust
use network_sim::{apply_network_params, NetworkParams, QdiscManager};

#[tokio::test]
async fn test_rist_with_network_simulation() -> Result<(), Box<dyn std::error::Error>> {
    let qdisc_manager = QdiscManager::default();

    // Apply asymmetric network conditions to sender/receiver paths
    apply_network_params(&qdisc_manager, "veth-sender", &NetworkParams::typical()).await?;
    apply_network_params(&qdisc_manager, "veth-receiver", &NetworkParams::poor()).await?;

    // Run your RIST tests here ...

    // Clean up after
    network_sim::remove_network_params(&qdisc_manager, "veth-sender").await?;
    network_sim::remove_network_params(&qdisc_manager, "veth-receiver").await?;
    Ok(())
}
```
```

## Network Profiles

The library provides three carefully tuned predefined network profiles based on real-world network conditions:

### Good Network Conditions
```rust
let params = NetworkParams::good();
// - Delay: 5ms (low-latency local network)
// - Loss: 0.1% (minimal packet loss)
// - Bandwidth: 10 Mbps (high-speed connection)
// Use case: Local LAN, fiber connections, ideal conditions
```

### Typical Network Conditions
```rust
let params = NetworkParams::typical();
// - Delay: 20ms (typical internet latency)
// - Loss: 1% (normal packet loss)
// - Bandwidth: 5 Mbps (standard broadband)
// Use case: Regular internet connections, cable/DSL
```

### Poor Network Conditions
```rust
let params = NetworkParams::poor();
// - Delay: 100ms (high latency)
// - Loss: 5% (significant packet loss)
// - Bandwidth: 1 Mbps (limited bandwidth)
// Use case: Satellite connections, congested networks, mobile networks
```

## Technical Implementation

### Linux Traffic Control Integration

The library uses Linux's Traffic Control (TC) via the `tc` command to apply network conditions:

- **Qdisc Management**: Creates and manages queueing disciplines on network interfaces
- **Netem Integration**: Uses Network Emulation (netem) qdisc for realistic network behavior
- **HTB Classful Shaping**: Applies rate limiting using an HTB root and class, attaching netem as a child under the class

### Architecture

```
Application Code
     ↓
network-sim API
     ↓
QdiscManager
Linux Kernel (TC subsystem)
     ↓
Network Interface (veth, eth, etc.)
```

## Error Handling

The library provides clear errors via `RuntimeError` and `QdiscError`:

```rust
use network_sim::{apply_network_params, NetworkParams, QdiscManager, RuntimeError};
use network_sim::qdisc::QdiscError;

#[tokio::main]
async fn main() {
    let qdisc_manager = QdiscManager::default();
    let params = NetworkParams::typical();

    match apply_network_params(&qdisc_manager, "nonexistent-iface", &params).await {
        Ok(_) => println!("Network parameters applied successfully"),
        Err(RuntimeError::Qdisc(QdiscError::InterfaceNotFound(iface))) => {
            eprintln!("Interface '{}' not found", iface);
        },
        Err(RuntimeError::Qdisc(QdiscError::PermissionDenied)) => {
            eprintln!("Need CAP_NET_ADMIN capability to modify network interfaces");
        },
        Err(RuntimeError::InvalidParams(msg)) => {
            eprintln!("Invalid parameters: {}", msg);
        },
        Err(e) => eprintln!("Other error: {}", e),
    }
}
```

### Error Types
- `InterfaceNotFound`: Specified network interface doesn't exist
- `PermissionDenied`: Missing required capabilities (CAP_NET_ADMIN)
- `InvalidParameters`: Parameter validation failed (negative delay, etc.)
- `CommandFailed`: Underlying `tc`/`ip` command failed (stderr included)

## Requirements and Compatibility

### System Requirements
- **Operating System**: Linux with Traffic Control support (kernel 2.6+)
- **Capabilities**: `CAP_NET_ADMIN` for interface modification
- **Dependencies**: iproute2 tools (`tc` and `ip`) available on PATH

### Rust Requirements
- **MSRV**: Rust 1.70+
- **Runtime**: Tokio 1.0+ for async operations
- **Platform**: Linux only (uses Linux-specific TC features)

### Container Environment
Works seamlessly in Docker containers with appropriate capabilities:

```dockerfile
# Dockerfile example
FROM rust:1.75

# Install required capabilities for network manipulation
RUN apt-get update && apt-get install -y iproute2

# Your application code
COPY . /app
WORKDIR /app

# Run container with network admin capabilities
# docker run --cap-add=NET_ADMIN --cap-add=SYS_ADMIN your-image
```

### Permission Setup
```bash
# For testing (temporary)
sudo -E cargo test

# For permanent capability grant
sudo setcap cap_net_admin+ep target/debug/your-binary

# For development containers (docker-compose.yml)
cap_add:
  - NET_ADMIN
  - SYS_ADMIN
```

## Advanced Usage

### Temporary Network Conditions

```rust
use network_sim::{apply_network_params, remove_network_params, NetworkParams, QdiscManager};
use tokio::time::{sleep, Duration};

async fn temporary_network_test() -> Result<(), Box<dyn std::error::Error>> {
    let qdisc_manager = QdiscManager::default();
    let interface = "veth-test";
    
    // Apply poor network conditions
    let params = NetworkParams::poor();
    apply_network_params(&qdisc_manager, interface, &params).await?;
    
    // Run your network-sensitive tests here
    println!("Running tests with poor network conditions...");
    sleep(Duration::from_secs(30)).await;
    
    // Remove network simulation (restore normal conditions)
    remove_network_params(&qdisc_manager, interface).await?;
    
    println!("Network conditions restored");
    Ok(())
}
```

### Monitoring Applied Conditions

```rust
use network_sim::qdisc::QdiscManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let qdisc_manager = QdiscManager::default();
    let iface = "veth0";
    let desc = qdisc_manager.describe_interface_qdisc(iface).await?;
    println!("qdisc tree for {}:\n{}", iface, desc);
    let stats = qdisc_manager.get_interface_stats(iface).await?;
    println!("sent={}B pkts={} dropped={}", stats.sent_bytes, stats.sent_packets, stats.dropped);
    Ok(())
}
```

### Ingress Shaping

For inbound conditions, the crate provides helpers that redirect ingress to an IFB device and apply shaping there:

```rust
use network_sim::{apply_ingress_params, remove_ingress_params, NetworkParams, QdiscManager};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let qdisc_manager = QdiscManager::default();
    let iface = "veth0";
    apply_ingress_params(&qdisc_manager, iface, &NetworkParams::typical()).await?;
    // ... run test traffic ...
    remove_ingress_params(&qdisc_manager, iface).await?;
    Ok(())
}
```

### Custom Qdisc Configuration

Advanced builders are not yet available. Prefer `NetworkParams` fields to control delay, loss, jitter, reorder, duplicate, and rate.

## Integration Examples

### With RIST Elements Testing

```rust
// Example integration test setup
use network_sim::{apply_network_params, NetworkParams, QdiscManager};

#[tokio::test]
async fn test_rist_with_network_simulation() -> Result<(), Box<dyn std::error::Error>> {
    let qdisc_manager = QdiscManager::default();
    
    // Set up network conditions for RIST testing
    apply_network_params(&qdisc_manager, "veth-sender", &NetworkParams::typical()).await?;
    apply_network_params(&qdisc_manager, "veth-receiver", &NetworkParams::poor()).await?;
    
    // Run RIST bonding tests with asymmetric network conditions
    // ... your RIST test code here ...
    
    Ok(())
}
```

### With Performance Benchmarking

```rust
use network_sim::{apply_network_params, NetworkParams, QdiscManager};
use std::time::Instant;

async fn benchmark_under_conditions(params: &NetworkParams) -> Result<Duration, Box<dyn std::error::Error>> {
    let qdisc_manager = QdiscManager::default();
    
    // Apply network conditions
    apply_network_params(&qdisc_manager, "test-iface", params).await?;
    
    // Run benchmark
    let start = Instant::now();
    
    // ... your benchmark code here ...
    
    let duration = start.elapsed();
    
    // Clean up
    remove_network_params(&qdisc_manager, "test-iface").await?;
    
    Ok(duration)
}
```

## Limitations

This library is intentionally focused on simplicity and specific use cases:

- **Static parameters only**: No support for time-varying network conditions
- **Linux-specific**: Uses Linux Traffic Control, not portable to other operating systems  
- **Basic qdisc support**: Focused on netem and HTB, doesn't expose full TC functionality
- **No complex scenarios**: For advanced network simulation, consider ns-3, mininet, or direct TC usage
- **Container-focused**: Designed for containerized testing, not production traffic shaping

For complex network simulation scenarios with time-varying conditions, Markov chains, or detailed protocol modeling, consider using dedicated network simulation frameworks.

## Planned rework: crate-managed namespaces and links

To reduce duplication and improve reliability of tests, the crate will expose higher-level APIs managing namespaces and links internally, so tests no longer need to call `ip netns ...` or `setns` directly.

- `Namespace` and `NamespaceGuard` for lifecycle and scoped entry
- `VethPairConfig`/`VethPair` for creating/managing veth pairs across namespaces
- `QdiscManager::*_in_ns` helpers to apply/inspect shaping within a namespace

See the design doc at `/workspace/specs/001-build-a-throughput/network-sim-rework.md` for details and the migration plan.

## Contributing

1. Fork the repository
2. Create a feature branch
3. Write tests for new functionality
4. Ensure all tests pass with network capabilities: `sudo -E cargo test`
5. Follow Rust coding standards and run `cargo fmt` and `cargo clippy`
6. Submit a pull request

### Development Setup

```bash
# Install required system packages (Ubuntu/Debian)
sudo apt-get install -y iproute2 libcap2-bin

# Grant capabilities for testing
sudo setcap cap_net_admin,cap_sys_admin+ep target/debug/deps/*

# Run tests
cargo test
```

## License

Licensed under the MIT License. See the [LICENSE](../../LICENSE) file for details.

## Project Integration

This crate is part of the [RIST Bonding](../../README.md) project. For complete documentation:

- **[Main Project Documentation](../../README.md)**: Overview and quick start guide
- **[Testing Guide](../../docs/testing/README.md)**: Comprehensive testing setup
- **[Plugin Documentation](../../docs/plugins/README.md)**: GStreamer element details
- **[Development Environment](../../docs/testing/DOCKER_TESTING.md)**: Container-based development
