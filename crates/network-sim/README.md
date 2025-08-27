# network-sim

Simple network simulation library for applying fixed network parameters to Linux network interfaces.

## Overview

This library provides a "set and forget" approach to network simulation. Instead of complex time-varying schedules or Markov chains, it simply applies static network parameters to network interfaces using Linux traffic control (qdisc).

## Features

- **Simple Parameter Application**: Apply fixed delay, loss, and rate limiting to network interfaces
- **Predefined Network Profiles**: Good, typical, and poor network conditions
- **Clean Error Handling**: Structured error types for different failure modes
- **Async Support**: Built with tokio for async network operations

## Usage

```rust
use network_sim::{apply_network_params, NetworkParams, QdiscManager};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let qdisc_manager = QdiscManager::default();
    let params = NetworkParams::typical(); // 20ms delay, 1% loss, 5Mbps
    
    apply_network_params(&qdisc_manager, "veth0", &params).await?;
    
    Ok(())
}
```

## Network Profiles

The library provides three predefined network profiles:

- **Good**: 5ms delay, 0.1% loss, 10 Mbps
- **Typical**: 20ms delay, 1% loss, 5 Mbps  
- **Poor**: 100ms delay, 5% loss, 1 Mbps

## Requirements

- Linux system with network namespaces support
- Root privileges for qdisc manipulation
- Rust 1.70+ with tokio runtime

## Note

This is a simplified library focused on applying static network parameters. For complex time-varying network simulation scenarios, consider using more sophisticated traffic control tools directly.
