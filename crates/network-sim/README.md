# network-sim

A simple yet powerful network simulation library for applying realistic network conditions to Linux network interfaces using Traffic Control (TC). Designed for testing RIST bonding and other network-sensitive applications in controlled environments.

## Overview

This library provides a "set and forget" approach to network simulation, focusing on applying static network parameters rather than complex time-varying scenarios. It integrates seamlessly with Linux's Traffic Control system to create realistic network conditions for testing and development.

**Key Philosophy**: Simple, predictable network parameter application with clean async APIs for modern Rust applications.

## Features

- **Static network parameter application**: Apply fixed delay, packet loss, and rate limiting to network interfaces
- **Predefined network profiles**: Pre-configured "good", "typical", and "poor" network conditions
- **Linux Traffic Control integration**: Uses kernel qdisc for realistic network behavior
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
    delay_ms: 50,        // 50ms one-way delay
    loss_percent: 2.5,   // 2.5% packet loss
    rate_mbps: Some(3.0), // 3 Mbps bandwidth limit
};

// Or use builder pattern (if implemented)
let params = NetworkParams::new()
    .with_delay(25)      // 25ms delay
    .with_loss(1.5)      // 1.5% loss
    .with_rate(8.0);     // 8 Mbps limit
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

### Integration with RIST Testing

```rust
// Example: Tests automatically manage network setup
#[tokio::test]
async fn test_rist_with_network_simulation() -> Result<(), Box<dyn std::error::Error>> {
    // Network setup is handled automatically by test framework
    let test_env = RistTestEnvironment::new().await?;
    
    // Apply network conditions through the test environment
    test_env.apply_conditions(NetworkParams::poor()).await?;
    
    // Run RIST streaming tests - all setup is automatic
    test_env.run_streaming_test(Duration::from_secs(30)).await?;
    
    // Cleanup is automatic when test_env is dropped
    Ok(())
}
```

The network-sim crate integrates seamlessly with the test framework to provide automated network condition testing without manual interface manipulation.
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

The library uses Linux's Traffic Control (TC) system through netlink sockets to apply network conditions:

- **Qdisc Management**: Creates and manages queueing disciplines on network interfaces
- **Netem Integration**: Uses Network Emulation (netem) qdisc for realistic network behavior
- **Token Bucket Filtering**: Applies rate limiting using TBF qdisc when bandwidth limits are specified
- **Netlink Communication**: Direct kernel communication for efficient parameter application

### Architecture

```
Application Code
     ↓
network-sim API
     ↓
QdiscManager
     ↓ 
netlink-packet-route
     ↓
Linux Kernel (TC subsystem)
     ↓
Network Interface (veth, eth, etc.)
```

## Error Handling

The library provides comprehensive error types for different failure scenarios:

```rust
use network_sim::{apply_network_params, NetworkParams, QdiscManager, NetworkSimError};

#[tokio::main]
async fn main() {
    let qdisc_manager = QdiscManager::default();
    let params = NetworkParams::typical();
    
    match apply_network_params(&qdisc_manager, "nonexistent-iface", &params).await {
        Ok(_) => println!("Network parameters applied successfully"),
        Err(NetworkSimError::InterfaceNotFound(iface)) => {
            eprintln!("Interface '{}' not found", iface);
        },
        Err(NetworkSimError::InsufficientPermissions) => {
            eprintln!("Need CAP_NET_ADMIN capability to modify network interfaces");
        },
        Err(NetworkSimError::InvalidParameters { param, reason }) => {
            eprintln!("Invalid parameter '{}': {}", param, reason);
        },
        Err(err) => eprintln!("Other error: {}", err),
    }
}
```

### Error Types
- `InterfaceNotFound`: Specified network interface doesn't exist
- `InsufficientPermissions`: Missing required capabilities (CAP_NET_ADMIN)
- `InvalidParameters`: Parameter validation failed (negative delay, etc.)
- `NetlinkError`: Low-level netlink communication error
- `QdiscOperationFailed`: Traffic control operation failed

## Requirements and Compatibility

### System Requirements
- **Operating System**: Linux with Traffic Control support (kernel 2.6+)
- **Capabilities**: `CAP_NET_ADMIN` for interface modification
- **Dependencies**: No external binaries required (uses netlink directly)

### Rust Requirements
- **MSRV**: Rust 1.70+
- **Runtime**: Tokio 1.0+ for async operations
- **Platform**: Linux only (uses Linux-specific netlink and TC features)

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
use network_sim::{get_interface_stats, QdiscManager};

async fn monitor_interface(interface: &str) -> Result<(), Box<dyn std::error::Error>> {
    let qdisc_manager = QdiscManager::default();
    
    let stats = get_interface_stats(&qdisc_manager, interface).await?;
    
    println!("Interface: {}", interface);
    println!("Packets sent: {}", stats.tx_packets);
    println!("Packets dropped: {}", stats.tx_dropped);
    println!("Bytes transmitted: {}", stats.tx_bytes);
    
    Ok(())
}
```

### Custom Qdisc Configuration

```rust
use network_sim::{QdiscManager, QdiscConfig};

async fn advanced_qdisc_setup() -> Result<(), Box<dyn std::error::Error>> {
    let qdisc_manager = QdiscManager::default();
    
    // Create custom qdisc configuration
    let config = QdiscConfig::new("netem")
        .with_delay(30) // 30ms delay
        .with_jitter(5) // ±5ms jitter
        .with_loss_correlation(25.0) // 25% loss correlation
        .with_reorder(10.0, 50.0); // 10% reorder probability, 50% correlation
    
    apply_custom_qdisc(&qdisc_manager, "veth0", &config).await?;
    
    Ok(())
}
```

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
- **Basic qdisc support**: Focused on netem and TBF, doesn't expose full TC functionality
- **No complex scenarios**: For advanced network simulation, consider ns-3, mininet, or direct TC usage
- **Container-focused**: Designed for containerized testing, not production traffic shaping

For complex network simulation scenarios with time-varying conditions, Markov chains, or detailed protocol modeling, consider using dedicated network simulation frameworks.

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
