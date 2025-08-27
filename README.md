# RIST Bonding

A high-performance RIST (Reliable Internet Stream Transport) bonding implementation using GStreamer, designed for resilient video streaming over multiple network paths.

## Features

- **Multi-path bonding**: Distribute traffic across multiple network links for increased throughput and reliability
- **Dynamic load balancing**: Automatically adjust traffic distribution based on real-time network conditions
- **Network simulation**: Built-in tools for testing under various network conditions (latency, packet loss, bandwidth)
- **GStreamer integration**: Native GStreamer elements for seamless pipeline integration
- **Docker-based testing**: Complete containerized testing environment with network namespace support
- **VS Code devcontainer**: Ready-to-use development environment

## Quick Start

### Development Environment Options

#### Option 1: VS Code Devcontainer (Recommended)
1. Install VS Code with the Remote-Containers extension
2. Clone the repository
3. Open in VS Code and select "Reopen in Container"
4. Everything is pre-configured and ready to use!

#### Option 2: Docker Development
```bash
# Clone the repository
git clone <repository-url>
cd rist-bonding

# Build and run tests
./scripts/docker-test.sh test

# Interactive development environment
./scripts/docker-test.sh dev
```

#### Option 3: Local Development
```bash
# Install dependencies (Ubuntu/Debian)
sudo apt update
sudo apt install -y \
    build-essential \
    pkg-config \
    libgstreamer1.0-dev \
    libgstreamer-plugins-base1.0-dev \
    libgstreamer-plugins-bad1.0-dev \
    gstreamer1.0-plugins-base \
    gstreamer1.0-plugins-good \
    gstreamer1.0-plugins-bad \
    gstreamer1.0-plugins-ugly \
    gstreamer1.0-libav

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build and test
cargo build --all-features
cargo test --all-features
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

### Using VS Code Devcontainer
1. Open the project in VS Code
2. Click "Reopen in Container" when prompted
3. Use the integrated tasks:
   - `Ctrl+Shift+P` → "Tasks: Run Task" → "Docker: Run All Tests"
   - `F5` to debug tests
   - `Ctrl+Shift+P` → "Tasks: Run Task" → "Docker: Setup Network Test Environment"

### Using Docker Scripts
```bash
# Run all tests
./scripts/docker-test.sh test

# Set up network simulation
./scripts/docker-test.sh network

# Interactive development
./scripts/docker-test.sh dev

# Build containers
./scripts/docker-test.sh build
```

### Using Development Helper (in devcontainer)
```bash
# Initial setup
./.devcontainer/dev-helper.sh setup

# Run tests
./.devcontainer/dev-helper.sh test

# Check network capabilities
./.devcontainer/dev-helper.sh network-check

# Generate documentation
./.devcontainer/dev-helper.sh docs
```

## Testing

The project includes comprehensive testing at multiple levels:

- **Unit Tests**: Test individual components in isolation
- **Integration Tests**: Test component interactions and GStreamer pipelines
- **Scenario Tests**: Test complete bonding scenarios under various conditions
- **Stress Tests**: Performance and reliability testing under load
- **Network Tests**: Docker-based network simulation tests

### Test Categories
```bash
# Unit tests
cargo test --lib

# Integration tests  
cargo test --test '*'

# Network simulation tests
cargo test -p network-sim --features docker

# All tests via Docker
./scripts/docker-test.sh test
```

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
5. Ensure all tests pass: `./scripts/docker-test.sh test`
6. Submit a pull request

### Code Style
- Run `cargo fmt` before committing
- Use `cargo clippy` to catch common issues
- Follow Rust naming conventions
- Document public APIs with rustdoc

## Documentation

Generate and view documentation:
```bash
# In devcontainer
./.devcontainer/dev-helper.sh docs

# Or directly with cargo
cargo doc --all-features --open
```

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- GStreamer community for the excellent multimedia framework
- RIST Forum for the RIST specification
- Contributors to the open-source networking tools used in testing