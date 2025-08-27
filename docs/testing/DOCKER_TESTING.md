# Development Container Guide

This document explains how to develop and test RIST bonding using the devcontainer environment with direct commands (no scripts).

## Quick Start

### Prerequisites

- VS Code with Remote-Containers extension
- Docker and Docker Compose installed

### Using the Development Container

```bash
# Open project in VS Code
code .

# Select "Reopen in Container" when prompted
# Or: Ctrl+Shift+P -> "Remote-Containers: Reopen in Container"
```

## Development Commands

### Basic Development

```bash
# Check Rust installation
cargo --version
rustc --version

# Build the project
cargo build --all-features

# Run all tests
cargo test --all-features

# Run specific test categories
cargo test --lib                    # Unit tests only
cargo test --test '*'               # Integration tests only
cargo test -p network-sim           # Network simulation tests
cargo test -p rist-elements         # RIST elements tests

# Code quality
cargo fmt --all                     # Format code
cargo clippy --all-targets --all-features -- -D warnings  # Linting

# Generate documentation
cargo doc --all-features --no-deps --open
```

### GStreamer Development

```bash
# Check GStreamer installation
gst-inspect-1.0 --version

# List available RIST plugins
gst-inspect-1.0 | grep -i rist

# Inspect specific RIST plugins
gst-inspect-1.0 ristsrc
gst-inspect-1.0 ristsink
```

## Local Network Testing

### Setting Up RIST Network Namespaces

For testing RIST communication locally within the container:

```bash
# Create network namespaces for isolated RIST endpoints
ip netns add rist-sender 2>/dev/null || true
ip netns add rist-receiver 2>/dev/null || true

# Create veth pairs for local RIST communication
ip link add veth-sender type veth peer name veth-sender-peer 2>/dev/null || true
ip link add veth-receiver type veth peer name veth-receiver-peer 2>/dev/null || true

# Move interfaces to respective namespaces
ip link set veth-sender-peer netns rist-sender 2>/dev/null || true
ip link set veth-receiver-peer netns rist-receiver 2>/dev/null || true

# Configure interfaces in root namespace (for routing between endpoints)
ip addr add 192.168.100.1/24 dev veth-sender 2>/dev/null || true
ip addr add 192.168.101.1/24 dev veth-receiver 2>/dev/null || true
ip link set veth-sender up 2>/dev/null || true
ip link set veth-receiver up 2>/dev/null || true

# Configure interfaces in sender namespace
ip netns exec rist-sender ip addr add 192.168.100.2/24 dev veth-sender-peer 2>/dev/null || true
ip netns exec rist-sender ip link set veth-sender-peer up 2>/dev/null || true
ip netns exec rist-sender ip link set lo up 2>/dev/null || true

# Configure interfaces in receiver namespace
ip netns exec rist-receiver ip addr add 192.168.101.2/24 dev veth-receiver-peer 2>/dev/null || true
ip netns exec rist-receiver ip link set veth-receiver-peer up 2>/dev/null || true
ip netns exec rist-receiver ip link set lo up 2>/dev/null || true

# Set up routing for RIST communication
ip netns exec rist-sender ip route add default via 192.168.100.1 2>/dev/null || true
ip netns exec rist-receiver ip route add default via 192.168.101.1 2>/dev/null || true
```

### Verifying Network Setup

```bash
# List network namespaces
ip netns list

# Check network interfaces
ip addr show | grep veth

# Test connectivity
ping -c 1 -W 1 192.168.100.2   # Test sender endpoint
ping -c 1 -W 1 192.168.101.2   # Test receiver endpoint
```

### Running RIST Tests in Namespaces

```bash
# Run RIST sender in sender namespace
ip netns exec rist-sender gst-launch-1.0 videotestsrc ! x264enc ! ristsink uri=rist://192.168.101.2:1968

# Run RIST receiver in receiver namespace (different terminal)
ip netns exec rist-receiver gst-launch-1.0 ristsrc uri=rist://0.0.0.0:1968 ! decodebin ! autovideosink
```

## Docker Container Management

### Direct Container Commands

```bash
# Build development container
docker-compose build rist-bonding-dev

# Start development container interactively
docker-compose run --rm rist-bonding-dev

# Run tests in testing container
docker-compose run --rm rist-bonding-test

# Execute commands in running container
docker-compose exec rist-bonding-dev cargo test --lib
```

### Container Resource Management

```bash
# Check container resource usage
docker stats

# Clean up containers and images
docker system prune -f

# View container logs
docker-compose logs rist-bonding-dev
```

## GitHub Actions Integration

The project uses GitHub Actions for automated CI/CD. All commands run in CI are direct commands:

### Main CI Commands (`ci.yml`)

```bash
# Code quality checks
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings

# Testing
cargo test --workspace --lib --locked --verbose
cargo test --workspace --test '*' --locked --verbose

# Container testing
docker-compose build rist-bonding-dev
docker-compose run --rm rist-bonding-test
```

### Docker Environment Validation (`docker-ci.yml`)

```bash
# Development environment validation
docker-compose run --rm rist-bonding-dev bash -c "cargo --version && gst-inspect-1.0 --version"

# Network capability testing
docker-compose run --rm rist-bonding-dev bash -c "ip netns add test && ip netns list && ip netns del test"
```

## Development Workflow

### 1. Container Development

```bash
# Open in VS Code devcontainer (automatic)
# Or manually start container:
docker-compose run --rm rist-bonding-dev

# Inside container:
cargo check --workspace    # Quick compile check
cargo test --lib          # Quick unit tests
```

### 2. Testing Changes

```bash
# Run comprehensive tests
cargo test --all-features

# Test specific components
cargo test -p network-sim --all-features
cargo test -p rist-elements

# Integration tests
cargo test --test integration_tests --all-features
```

### 3. Network Testing

```bash
# Set up network environment (copy commands from above)
ip netns add rist-sender
# ... (full setup commands)

# Run network-specific tests
cargo test -p network-sim

# Test RIST communication
# (run sender and receiver commands in different terminals)
```

### 4. Code Quality

```bash
# Format code
cargo fmt --all

# Check for issues
cargo clippy --all-targets --all-features -- -D warnings

# Security audit
cargo audit
```

## Network Architecture

### Local RIST Communication

All RIST operations happen locally within the container:

- **Sender Endpoint**: `rist-sender` namespace (192.168.100.2)
- **Receiver Endpoint**: `rist-receiver` namespace (192.168.101.2)
- **Network Simulation**: Handled by `network-sim` crate
- **No External Network**: All communication is container-internal

### Example Network Test

```rust
#[tokio::test]
async fn test_local_rist_communication() {
    // Network setup would be done via network-sim crate
    let sender_addr = "192.168.100.2:1968";
    let receiver_addr = "192.168.101.2:1968";
    
    // Test implementation using network-sim
    // All setup done programmatically, no scripts
}
```

## Troubleshooting

### Permission Issues

```bash
# Verify container has NET_ADMIN capability
docker inspect rist-bonding-dev | grep -A 5 CapAdd

# Test network namespace creation
ip netns add test-ns && ip netns del test-ns
```

### Network Setup Issues

```bash
# Check if network interfaces exist
ip addr show | grep veth

# Verify namespaces are created
ip netns list

# Test basic connectivity
ping -c 1 127.0.0.1
```

### Build Issues

```bash
# Update Rust toolchain
rustup update stable

# Clean build artifacts
cargo clean

# Rebuild with verbose output
cargo build --verbose --all-features
```

### VS Code Integration Issues

```bash
# Rebuild container
docker-compose build rist-bonding-dev

# Check container status
docker-compose ps
```

## Performance Considerations

- Container startup: ~2-3 seconds
- Network namespace setup: ~100ms per command
- Test execution: Comparable to native performance
- Memory usage: ~500MB for development container

## Best Practices

1. **Use direct cargo commands** - No wrapper scripts needed
2. **Set up network namespaces manually** - Copy/paste commands as needed
3. **Test locally before pushing** - Use `cargo test --all-features`
4. **Use GitHub Actions for CI** - Automated validation on push
5. **Keep network tests isolated** - All RIST communication stays local
6. **Document command sequences** - Keep track of complex setups

This approach provides complete control over the development environment using direct commands without any wrapper scripts.