# RIST Bonding

A high-performance RIST (Reliable Internet Stream Transport) bonding implementation using GStreamer, designed for resilient video streaming over multiple network paths with comprehensive local network simulation capabilities.

## Features

- **Multi-path bonding**: Distribute RTP traffic across multiple network links using intelligent weighted round-robin
- **Dynamic load balancing**: Automatically adjust traffic distribution based on real-time RIST statistics (RTT, packet loss, retransmissions)
- **Advanced GStreamer elements**: Custom `ristdispatcher` and `dynbitrate` elements for sophisticated stream control
- **Network simulation**: Built-in tools for realistic network condition testing using Linux traffic control
- **Local RIST testing**: Complete network namespace setup for local RIST sender/receiver testing
- **Containerized development**: VS Code devcontainer with all dependencies pre-configured
- **Automated CI/CD**: GitHub Actions for comprehensive testing and validation

## Quick Start

### Development Environment (VS Code Devcontainer)

1. **Prerequisites**: VS Code with Dev Containers extension, Docker
2. **Setup**:
   ```bash
   git clone https://github.com/RephlexZero/rist-bonding.git
   cd rist-bonding
   code .
   # Select "Reopen in Container" when prompted
   ```
3. **Development**: Everything is pre-configured with GStreamer, Rust, and network tools!

The devcontainer provides a complete development environment with all dependencies pre-installed and configured.

## Project Structure

```
rist-bonding/
├── .devcontainer/          # VS Code devcontainer configuration
├── .github/                # GitHub Actions workflows
├── crates/
│   ├── rist-elements/      # Custom GStreamer elements (ristdispatcher, dynbitrate)
│   └── network-sim/        # Network simulation library using Linux TC
├── docs/                   # Documentation
│   ├── plugins/            # GStreamer plugin documentation
│   ├── testing/            # Testing guides and setup
│   └── visualization/      # Network visualization tools (future)
├── gstreamer/              # GStreamer submodule (upstream)
├── rist/                   # RIST plugin C implementation
├── scripts/                # Build and deployment scripts
├── target/                 # Build artifacts and documentation
├── docker-compose.yml      # Container orchestration
├── Dockerfile              # Development container definition
├── Cargo.toml              # Workspace configuration
└── Makefile                # Docker build automation
```

## Core Components

### GStreamer Elements

#### RistDispatcher (`ristdispatcher`)
A sophisticated multi-output dispatcher implementing RIST bonding concepts:
- **Smart routing**: Smooth Weighted Round Robin (SWRR) packet distribution
- **Automatic rebalancing**: Adjusts weights based on real-time RIST statistics
- **Failover support**: Detects link failures and redistributes traffic
- **Keyframe duplication**: Optional duplication across links for faster recovery
- **Request pads**: Dynamic source pad creation (`src_%u`)

Properties:
- `weights`: Per-pad traffic distribution weights (array of floats)
- `auto-balance`: Enable automatic weight adjustment based on statistics
- `failover-timeout`: Link failure detection timeout (milliseconds)
- `keyframe-duplicate`: Duplicate keyframes across active links

#### DynBitrate (`dynbitrate`) 
A control element for adaptive bitrate management:
- **Statistics-driven**: Uses RIST packet loss and RTT metrics
- **Encoder integration**: Drives upstream encoder bitrate property
- **Gentle adjustment**: Configurable step sizes and rate limiting
- **Dispatcher coordination**: Optionally controls ristdispatcher weights

Properties:
- `target-loss`: Target packet loss percentage (0.0-100.0)
- `target-rtt`: Target round-trip time in milliseconds
- `step-size`: Bitrate adjustment step size percentage
- `dispatcher`: Reference to ristdispatcher for unified control

### Network Simulation

#### Network-Sim Crate
Provides realistic network condition simulation using Linux Traffic Control:
- **Fixed parameters**: Apply static delay, loss, and bandwidth limits
- **Predefined profiles**: Good (5ms, 0.1% loss), Typical (20ms, 1% loss), Poor (100ms, 5% loss)
- **Traffic control integration**: Uses Linux qdisc for realistic network behavior
- **Async support**: Tokio-based async API for integration with test frameworks

### RIST Protocol Integration

Built on GStreamer's RIST plugin with enhancements for multi-path operation:
- **Standard compliance**: Implements RIST Simple Profile with extensions
- **Statistics collection**: Per-session RTT, packet loss, and retransmission metrics
- **Bonding extensions**: Multi-session coordination and traffic distribution
- **Reliability features**: Automatic retransmission and packet recovery

## Development Workflow

### Using VS Code Devcontainer
1. Open the project in VS Code
2. Click "Reopen in Container" when prompted
3. All dependencies are pre-configured! Use direct cargo commands:
   ```bash
   # Build project
   cargo build --all-features
   
   # Run all tests
   cargo test --all-features
   
   # Run specific crate tests
   cargo test -p network-sim
   cargo test -p rist-elements
   
   # Code quality
   cargo fmt --all
   cargo clippy --all-targets --all-features -- -D warnings
   
   # Generate documentation
   cargo doc --all-features --no-deps --open
   ```

### Testing with Network Simulation
The project includes comprehensive testing through the crates without manual network setup:
```bash
# Run network simulation tests (handles setup automatically)
cargo test -p network-sim --all-features

# Run RIST element integration tests (creates test environment internally)
cargo test -p rist-elements --all-features

# Run specific integration tests with elevated privileges if needed
sudo -E cargo test -p rist-elements --test integration_tests -- --nocapture
```

All network namespaces, interfaces, and test environments are managed automatically by the test crates.

## Testing

### Test Categories
```bash
# Unit tests (fast, no external dependencies)
cargo test --lib

# Integration tests (with GStreamer and network components)
cargo test --test '*' --all-features

# Network simulation tests (requires network namespaces)
cargo test -p network-sim --all-features

# RIST element tests (requires RIST plugins)
cargo test -p rist-elements --all-features

# All tests with full features
cargo test --all-features
```

### Test Environment Setup
The devcontainer includes the necessary capabilities for network namespace creation. Some integration tests may require elevated privileges:

```bash
# Run network integration tests with sudo if needed
sudo -E cargo test -p rist-elements --test integration_tests -- --nocapture
```

### Continuous Integration
- **GitHub Actions**: Automated testing on push and pull requests
- **Multi-platform**: Tests run on Ubuntu with full GStreamer stack
- **Code quality**: Formatting, linting, security audits, and documentation checks
- **Container validation**: Ensures devcontainer builds and works correctly

### Test Environment Variables
- `RIST_SHOW_VIDEO=1`: Display video preview during integration tests
- `RIST_REQUIRE_BUFFERS=1`: Fail tests if no buffers are observed
- `GST_DEBUG=rist*:5`: Enable detailed RIST element logging
- `RUST_LOG=debug`: Enable detailed Rust logging

## Configuration

### Environment Variables
- `RUST_LOG`: Set logging level (trace, debug, info, warn, error)
- `RUST_BACKTRACE`: Enable backtraces (0, 1, full)
- `GST_DEBUG`: GStreamer debug level and categories (e.g., `rist*:5,dispatcher*:4`)
- `GST_PLUGIN_PATH`: Additional GStreamer plugin search paths

### GStreamer Element Properties

#### RistDispatcher
- `weights`: Array of per-pad weights for traffic distribution (default: `[1.0]`)
- `auto-balance`: Enable automatic weight adjustment (default: `true`)
- `failover-timeout`: Link failure detection timeout in ms (default: `5000`)
- `rebalance-interval`: Statistics update interval in ms (default: `1000`)
- `keyframe-duplicate`: Duplicate keyframes across links (default: `false`)
- `enable-stats`: Enable detailed statistics collection (default: `false`)

#### DynBitrate
- `target-loss`: Target packet loss percentage (default: `1.0`)
- `target-rtt`: Target RTT in milliseconds (default: `100`)
- `step-size`: Bitrate adjustment step percentage (default: `10.0`)
- `min-bitrate`: Minimum allowed bitrate in bps (default: `100000`)
- `max-bitrate`: Maximum allowed bitrate in bps (default: `10000000`)
- `dispatcher`: Reference to ristdispatcher element for coordination

### Network Simulation Profiles
```rust
// Predefined network profiles in network-sim crate
NetworkParams::good()    // 5ms delay, 0.1% loss, 10 Mbps
NetworkParams::typical() // 20ms delay, 1% loss, 5 Mbps
NetworkParams::poor()    // 100ms delay, 5% loss, 1 Mbps

// Custom parameters
NetworkParams {
    delay_ms: 25,
    loss_percent: 2.5,
    rate_mbps: Some(8.0),
}
```

## GStreamer Pipeline Examples

### Basic RIST Bonding Pipeline
```bash
# Simple dual-path bonding with equal weights
gst-launch-1.0 \
  videotestsrc pattern=ball ! \
  x264enc bitrate=2000 ! \
  rtph264pay ! \
  ristdispatcher name=d \
  d.src_0 ! ristsink uri=rist://192.168.1.10:1968 \
  d.src_1 ! ristsink uri=rist://192.168.1.11:1968
```

### Advanced Bonding with Custom Weights
```bash
# 70%/30% traffic distribution with automatic rebalancing
gst-launch-1.0 \
  videotestsrc pattern=ball ! \
  x264enc bitrate=5000 ! \
  rtph264pay ! \
  ristdispatcher weights="0.7,0.3" auto-balance=true name=d \
  d.src_0 ! ristsink uri=rist://primary.example.com:1968 \
  d.src_1 ! ristsink uri=rist://backup.example.com:1968
```

### Dynamic Bitrate Control with Bonding
```bash
# Adaptive bitrate with dispatcher coordination
gst-launch-1.0 \
  videotestsrc pattern=ball ! \
  x264enc name=encoder bitrate=3000 ! \
  dynbitrate target-loss=1.0 target-rtt=50 dispatcher=d ! \
  rtph264pay ! \
  ristdispatcher name=d auto-balance=false \
  d.src_0 ! ristsink uri=rist://192.168.1.10:1968 \
  d.src_1 ! ristsink uri=rist://192.168.1.11:1968
```

### Receiver Pipeline
```bash
# Simple RIST receiver with display
gst-launch-1.0 \
  ristsrc uri=rist://0.0.0.0:1968 ! \
  rtph264depay ! \
  avdec_h264 ! \
  videoconvert ! \
  autovideosink

# Receiver with statistics monitoring
gst-launch-1.0 \
  ristsrc uri=rist://0.0.0.0:1968 stats-print-interval=5000 ! \
  rtph264depay ! \
  avdec_h264 ! \
  fpsdisplaysink sync=false
```

## Architecture

### Bonding Strategy
1. **Smart packet distribution**: Uses Smooth Weighted Round Robin (SWRR) algorithm for optimal load balancing
2. **Real-time quality monitoring**: Continuously monitors RIST statistics (RTT, packet loss, retransmissions)
3. **Dynamic weight adjustment**: Automatically adjusts traffic distribution based on link performance metrics
4. **Intelligent failover**: Detects link failures and seamlessly redistributes traffic to healthy paths
5. **Keyframe optimization**: Optional keyframe duplication across links for faster stream recovery

### Network Simulation Architecture
1. **Linux Traffic Control integration**: Uses qdisc for realistic network condition simulation
2. **Network namespace isolation**: Each test scenario runs in isolated network environments
3. **Parameterized testing**: Fixed delay, loss, and bandwidth profiles for reproducible testing
4. **Async operation**: Tokio-based async networking for efficient resource utilization
5. **Container-local testing**: All RIST operations contained within development environment

### Element Interaction Flow
```
[Video Source] → [Encoder] → [DynBitrate] → [RTP Payloader] → [RistDispatcher]
                      ↓                                              ↓
                [Bitrate Control]                            [Multi-path Distribution]
                      ↓                                              ↓
              [Statistics Monitor] ←→ [RIST Statistics] ←→ [Weight Calculator]
                                                               ↓
                                                    [RistSink × N paths]
```

### Performance Characteristics
- **Latency**: Low-latency packet forwarding suitable for live streaming
- **Throughput**: Tested with high-bitrate streams (>100 Mbps aggregate)
- **Memory efficiency**: Zero-copy packet forwarding where possible
- **CPU optimization**: Efficient weight calculations and statistics processing

## Documentation

Detailed documentation is available in the `docs/` directory:

- **[Plugin Documentation](docs/plugins/README.md)**: Comprehensive guide to GStreamer elements
- **[Testing Guide](docs/testing/README.md)**: Complete testing setup and troubleshooting
- **[Docker Testing](docs/testing/DOCKER_TESTING.md)**: Container-based development and testing
- **[Network Visualization](docs/visualization/README.md)**: Future monitoring and visualization features
- **[Crate Documentation](crates/)**: Individual crate README files and API docs

Generate and view API documentation:
```bash
cargo doc --all-features --no-deps --open
```

## Contributing

1. Fork the repository
2. Create a feature branch
3. Use the VS Code devcontainer for development
4. Write tests for new functionality
5. Ensure all tests pass: `cargo test --all-features`
6. Push changes - GitHub Actions will validate your contribution

### Code Style
- Run `cargo fmt --all` before committing
- Use `cargo clippy --all-targets --all-features -- -D warnings` to catch issues
- Follow Rust naming conventions
- Document public APIs with rustdoc

### Local Development
- All development happens in the devcontainer with pre-configured environment
- Network testing is handled automatically by the test crates
- Use direct cargo commands for all operations
- GitHub Actions handle comprehensive CI/CD

## Documentation

Generate and view documentation:
```bash
# Generate documentation
cargo doc --all-features --no-deps --open

# Documentation will be available in target/doc/
```

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- **GStreamer community** for the excellent multimedia framework and plugin architecture
- **RIST Forum** for the RIST specification and reference implementations
- **Linux networking community** for Traffic Control and network namespace capabilities
- **Rust community** for the excellent ecosystem of crates used in this project
- **Contributors** to the open-source networking and multimedia tools that make this project possible