# Testing Guide

The RIST Bonding project uses a comprehensive testing framework with multiple test categories and environments. This guide covers all testing approaches from unit tests to integration tests with network simulation.

## Test Architecture Overview

The testing system is designed around several key principles:

- **Isolated testing environments**: Network namespaces prevent interference
- **Realistic network conditions**: Linux Traffic Control for authentic simulation  
- **Multiple test levels**: Unit, integration, and end-to-end testing
- **Container-based consistency**: All tests run in controlled Docker environments
- **Automated validation**: GitHub Actions for continuous integration

## Prerequisites

### System Requirements
- **GStreamer**: Full installation with RIST plugin support
- **Network capabilities**: `CAP_SYS_ADMIN`, `CAP_NET_ADMIN` for namespace creation
- **Container support**: Docker and Docker Compose for isolated testing
- **Build tools**: Rust toolchain with cargo test framework

### GStreamer Components
The test suite requires specific GStreamer elements:

```bash
# Verify required elements are available
gst-inspect-1.0 ristsrc      # RIST stream source
gst-inspect-1.0 ristsink     # RIST stream sink  
gst-inspect-1.0 x264enc      # H.264 encoder for test streams
gst-inspect-1.0 rtph264pay   # RTP H.264 payloader

# Verify test plugin elements (built with test-plugin feature)
gst-inspect-1.0 counter_sink # Buffer counting sink
```

### Development Environment Setup
```bash
# Using VS Code devcontainer (recommended)
code .  # Open in VS Code, select "Reopen in Container"
```

The devcontainer provides all necessary dependencies and capabilities pre-configured.

## Running Tests

### Basic Test Execution

Most tests can be run with standard cargo commands:

```bash
# Run all tests (unit + integration)
cargo test --all-features

# Unit tests only (fast, no external dependencies)
cargo test --lib

# Integration tests only  
cargo test --test '*' --all-features

# Specific crate tests
cargo test -p network-sim --all-features
cargo test -p rist-elements --all-features

# Test with detailed output
cargo test --all-features -- --nocapture
```

### Network Namespace Tests

Some integration tests require Linux network namespaces and elevated privileges within the devcontainer:

```bash
# Run network-dependent tests with sudo
sudo -E cargo test -p rist-elements --test integration_tests -- --nocapture

# Preserve environment variables with -E
sudo -E GST_DEBUG=rist*:5 cargo test -p rist-elements --test integration_tests
```

The devcontainer is configured with the necessary network capabilities (`CAP_NET_ADMIN`, `CAP_SYS_ADMIN`) to create and manage network namespaces.

### Test Categories

#### Unit Tests (`cargo test --lib`)
```bash
# Fast tests with no external dependencies
cargo test --lib -p network-sim        # Network simulation logic
cargo test --lib -p rist-elements      # Element behavior and properties
```

#### Integration Tests (`cargo test --test '*'`)
```bash
# GStreamer element integration
cargo test --test integration_tests -p rist-elements

# End-to-end RIST communication  
cargo test --test automated_integration -p rist-elements

# Network simulation integration
cargo test --test network_integration -p network-sim
```

#### Performance Tests
```bash
# Throughput and latency benchmarks
cargo test --test performance_tests -p rist-elements --release

# Network simulation performance
cargo test --test simulation_benchmarks -p network-sim --release
```

## Test Environment Variables

Configure test behavior and debugging through environment variables:

### Video and Display Control
```bash
# Show video preview during integration tests (requires display)
export RIST_SHOW_VIDEO=1

# Require buffer flow validation (fail if no buffers observed) 
export RIST_REQUIRE_BUFFERS=1

# Set video test duration (seconds)
export RIST_TEST_DURATION=30
```

### Debugging and Logging
```bash
# Enable GStreamer debug output
export GST_DEBUG=rist*:5,dispatcher*:4             # RIST and dispatcher elements
export GST_DEBUG=*:3                               # All elements at info level
export GST_DEBUG=rist*:5,*dispatcher*:4,*bitrate*:4  # Multiple element patterns

# Enable Rust logging
export RUST_LOG=debug                              # All debug messages
export RUST_LOG=network_sim=trace,rist_elements=debug  # Per-crate levels
export RUST_LOG=info                               # Info and above only

# Enable backtrace on panics
export RUST_BACKTRACE=1                           # Short backtrace
export RUST_BACKTRACE=full                        # Detailed backtrace
```

### Network Testing Control
```bash
# Force specific network simulation profiles
export NETWORK_SIM_PROFILE=poor                   # Use poor network conditions
export NETWORK_SIM_PROFILE=typical                # Use typical conditions

# Override network namespace names (for debugging)
export RIST_SENDER_NS=debug-sender
export RIST_RECEIVER_NS=debug-receiver

# Test timeout overrides
export RIST_CONNECT_TIMEOUT=10000                 # Connection timeout (ms)  
export RIST_STATS_INTERVAL=1000                   # Statistics polling interval (ms)
```

## Network Setup for Testing

### Automated Test Environment

Integration tests automatically create and manage isolated network environments using Linux network namespaces. The test crates handle all setup and cleanup:

```bash
# Network simulation tests (automatic setup/cleanup)
cargo test -p network-sim --all-features

# RIST integration tests (automatic namespace management)
cargo test -p rist-elements --test integration_tests --all-features
```

## Troubleshooting

### Common Issues and Solutions

#### Permission-Related Errors

**Error**: "Permission denied" when creating network namespaces
```bash
# Solution: Run with sudo or grant capabilities
sudo -E cargo test -p rist-elements --test integration_tests

# Or grant capabilities permanently
sudo setcap cap_sys_admin,cap_net_admin+ep target/debug/deps/integration_tests-*
```

**Error**: "Operation not permitted" for network operations
```bash
# Verify current capabilities  
getcap target/debug/deps/integration_tests-*

# Check if running in container with proper capabilities
docker run --cap-add=NET_ADMIN --cap-add=SYS_ADMIN ...
```

#### GStreamer Element Issues

**Error**: Element 'ristsrc' not found
```bash
# Verify RIST plugin installation
gst-inspect-1.0 ristsrc
gst-inspect-1.0 ristsink

# Check plugin path
export GST_DEBUG=GST_PLUGIN_LOADING:5
gst-inspect-1.0 ristsrc 2>&1 | grep -i rist
```

**Error**: Element 'counter_sink' not found  
```bash
# Ensure test-plugin feature is enabled
cargo build --features test-plugin

# Verify test plugin is available
export GST_PLUGIN_PATH=target/debug  
gst-inspect-1.0 counter_sink
```

#### Network Testing Issues

**Error**: Tests fail with network namespace creation errors
```bash
# Verify container has proper capabilities
docker inspect <container_id> | grep -A 5 CapAdd

# Test if namespace creation works
sudo -E cargo test -p network-sim --test basic_namespace_test
```

**Error**: RIST connection timeouts in tests
```bash
# Run tests with detailed logging to see network setup
export GST_DEBUG=rist*:5
export RUST_LOG=debug
cargo test -p rist-elements --test integration_tests -- --nocapture
```

**Error**: "No such device" errors in tests
```bash
# This indicates test cleanup issues - tests should handle this automatically
# If persistent, restart the devcontainer
```

#### Test Execution Issues

**Error**: Tests hang or timeout
```bash
# Enable debug logging to see where tests hang
export GST_DEBUG=rist*:5,*dispatcher*:4
export RUST_LOG=debug
cargo test --test integration_tests -- --nocapture

# Check if network cleanup is needed
ip netns del rist-sender rist-receiver 2>/dev/null || true
```

**Error**: "No buffers observed" in integration tests
```bash
# Extend test duration
export RIST_TEST_DURATION=60

# Enable buffer requirement for debugging
export RIST_REQUIRE_BUFFERS=1

# Check GStreamer pipeline with detailed debugging
export GST_DEBUG=4
```

#### Container-Specific Issues

**Error**: devcontainer fails to start
```bash
# Check Docker daemon status
systemctl status docker

# Rebuild container from scratch
docker-compose down && docker-compose build --no-cache rist-bonding-dev

# Check container logs
docker-compose logs rist-bonding-dev
```

**Error**: Network capabilities not available in devcontainer
```bash
# Verify container has NET_ADMIN capability
docker inspect <container_id> | grep -A 5 CapAdd

# Test network namespace creation
ip netns add test-ns && ip netns del test-ns
```

### Debugging Test Failures

#### Enable Comprehensive Logging
```bash
# Set up full debugging environment
export RUST_LOG=debug
export RUST_BACKTRACE=full
export GST_DEBUG=rist*:5,*dispatcher*:4,*bitrate*:4
export RIST_SHOW_VIDEO=1
export RIST_REQUIRE_BUFFERS=1

# Run specific failing test
cargo test --test integration_tests test_specific_failing_function -- --nocapture
```

#### Test-Specific Debugging

```bash
# Run single test with maximum verbosity
RUST_LOG=trace GST_DEBUG=6 cargo test --test integration_tests \
  test_dispatcher_basic_functionality -- --nocapture

# Run tests with custom timeout
RIST_CONNECT_TIMEOUT=30000 cargo test --test integration_tests

# Enable additional test diagnostics
RIST_SHOW_VIDEO=1 RIST_REQUIRE_BUFFERS=1 cargo test --test integration_tests
```

All network setup, RIST communication, and cleanup is handled automatically by the test framework.

### Performance Monitoring During Tests

```bash  
# Monitor system resources during test execution
watch -n 1 "ps aux | grep -E '(gst-launch|cargo|test)' | head -10"

# Monitor network interfaces
watch -n 1 "ip addr show | grep -A 3 veth"

# Monitor network statistics
watch -n 1 "ip netns exec rist-sender ip -s link show veth-sender-peer"
```

### Test Development and Debugging

When developing new tests or debugging existing ones:

1. **Start with unit tests**: Ensure basic functionality works
2. **Add logging early**: Use tracing/log crates for detailed output
3. **Test network setup separately**: Verify namespace creation before RIST tests
4. **Use manual pipelines**: Test GStreamer functionality independently
5. **Check resource cleanup**: Ensure tests don't leak namespaces or processes
6. **Test in container**: Ensure tests work in CI environment

For additional debugging help, see:

- **[Main Project Documentation](../../README.md)**: Overview and quick start
- **[Plugin Documentation](../plugins/README.md)**: Element-specific configuration and troubleshooting
- **[Docker Testing Guide](DOCKER_TESTING.md)**: Container-based development and testing
- **[Network Simulation](../../crates/network-sim/README.md)**: Network condition simulation details
- **[Integration Tests](../../crates/rist-elements/tests/README.md)**: Comprehensive test examples and patterns
