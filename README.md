# RIST Bonding

A high-performance RIST (Reliable Internet Stream Transport) bonding implementation using GStreamer, designed for resilient video streaming over multiple network paths with local network simulation.

## Features

- **Multi-path bonding**: Distribute traffic across multiple network links for increased throughput and reliability
- **Dynamic load balancing**: Automatically adjust traffic distribution based on real-time network conditions
- **Local network simulation**: Built-in tools for testing under various network conditions within containers
- **GStreamer integration**: Native GStreamer elements for seamless pipeline integration
- **Devcontainer development**: Complete containerized development environment
- **GitHub Actions CI**: Automated testing and validation

## Quick Start

### Development Environment (Recommended: VS Code Devcontainer)

1. **Prerequisites**: VS Code with Remote-Containers extension, Docker
2. **Setup**:
   ```bash
   git clone <repository-url>
   cd rist-bonding
   code .
   # Select "Reopen in Container" when prompted
   ```
3. **Development**: Everything is pre-configured and ready to use!

### Alternative: Docker Development

```bash
# Build and start development container
docker-compose build rist-bonding-dev
docker-compose run --rm rist-bonding-dev

# Inside container
./.devcontainer/dev-helper.sh setup
./.devcontainer/dev-helper.sh test
```

## Project Structure

```
rist-bonding/
├── .devcontainer/          # VS Code devcontainer configuration
├── crates/
│   ├── rist-elements/      # Core GStreamer elements
│   ├── network-sim/        # Network simulation tools
│   ├── scenarios/          # Test scenarios
│   └── demos/              # Example applications
├── docs/                   # Documentation
├── scripts/                # Build and test scripts
├── gstreamer/              # GStreamer submodule
└── docker-compose.yml      # Container orchestration
```

## Core Components

### RIST Elements
- **Dispatcher**: Distributes packets across multiple output pads based on configurable weights
- **DynBitrate**: Dynamically adjusts bitrate based on network conditions
- **RIST Transport**: Integration with GStreamer RIST plugin for reliable transport

### Network Simulation
- **Traffic Control**: Apply latency, packet loss, and bandwidth limitations
- **Network Namespaces**: Isolated network environments for testing
- **Docker Integration**: Containerized simulation with full network capabilities

## Development Workflow

### Using VS Code Devcontainer (Recommended)
1. Open the project in VS Code
2. Click "Reopen in Container" when prompted
3. Use direct cargo commands:
   ```bash
   # Build project
   cargo build --all-features
   
   # Run all tests
   cargo test --all-features
   
   # Run specific tests
   cargo test -p network-sim
   cargo test --lib
   
   # Code quality
   cargo fmt --all
   cargo clippy --all-targets --all-features -- -D warnings
   
   # Generate documentation
   cargo doc --all-features --no-deps --open
   ```

### Direct Container Development
```bash
# Start development container
docker-compose run --rm rist-bonding-dev

# Inside container - use direct cargo commands
cargo test --all-features
```

### Local Network Testing
Set up RIST network namespaces manually:
```bash
# Create namespaces
ip netns add rist-sender
ip netns add rist-receiver

# Create veth pairs
ip link add veth-sender type veth peer name veth-sender-peer
ip link add veth-receiver type veth peer name veth-receiver-peer

# Configure interfaces (see documentation for full setup)
ip netns exec rist-sender ip addr add 192.168.100.2/24 dev veth-sender-peer
ip netns exec rist-receiver ip addr add 192.168.101.2/24 dev veth-receiver-peer

# Run tests
cargo test -p network-sim --all-features
```

## Testing

The project uses GitHub Actions for automated testing with local development in devcontainers:

### Test Categories
```bash
# Unit tests (in devcontainer)
cargo test --lib

# Integration tests  
cargo test --test '*'

# Network simulation tests (local RIST endpoints)
cargo test -p network-sim --all-features

# All tests at once
cargo test --all-features
```

### Continuous Integration
- **GitHub Actions**: Automated testing on push/PR
- **Code Quality**: Formatting, linting, and security audits
- **Container Validation**: Ensures devcontainer works correctly
- **Local Network Testing**: All RIST operations tested within containers

### Network Architecture
- **Local RIST Endpoints**: Sender and receiver run in separate namespaces
- **Container-Internal**: No external network dependencies
- **Simulated Conditions**: Network-sim crate controls packet flow and conditions

## Configuration

### Environment Variables
- `RUST_LOG`: Set logging level (debug, info, warn, error)
- `RUST_BACKTRACE`: Enable backtraces (0, 1, full)
- `GST_DEBUG`: GStreamer debug level (0-9)

### GStreamer Pipeline Examples
```bash
# Basic bonding pipeline
gst-launch-1.0 \
  videotestsrc ! \
  dispatcher name=d \
  d.src_0 ! ristsink uri=rist://192.168.1.10:1968 \
  d.src_1 ! ristsink uri=rist://192.168.1.11:1968

# With dynamic bitrate control
gst-launch-1.0 \
  videotestsrc ! \
  dynbitrate ! \
  dispatcher name=d \
  d.src_0 ! ristsink uri=rist://192.168.1.10:1968 \
  d.src_1 ! ristsink uri=rist://192.168.1.11:1968
```

## Architecture

### Bonding Strategy
1. **Traffic Distribution**: Packets are distributed across multiple output paths using configurable weights
2. **Quality Monitoring**: Network quality metrics are continuously monitored via RIST statistics
3. **Dynamic Rebalancing**: Weights are adjusted based on real-time performance metrics
4. **Failure Recovery**: Automatic failover when paths become unavailable

### Network Simulation
1. **Namespace Isolation**: Each test scenario runs in isolated network namespaces
2. **Traffic Control**: Linux TC (Traffic Control) applies realistic network conditions
3. **Monitoring**: Comprehensive metrics collection and analysis
4. **Reproducibility**: Deterministic test scenarios for consistent results

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
- All development should happen in the devcontainer
- RIST operations are tested locally within the container using network namespaces
- Use direct cargo commands - no wrapper scripts
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

- GStreamer community for the excellent multimedia framework
- RIST Forum for the RIST specification
- Contributors to the open-source networking tools used in testing