# RIST Bonding Plugin Test Infrastructure

This directory contains comprehensive testing infrastructure for the RIST bonding GStreamer plugin, designed to simulate real-world 4G/5G cellular network conditions and stress test the bonding capabilities.

## Overview

The test infrastructure provides:

- **Automated CI/CD Pipeline**: GitHub Actions workflow for continuous testing
- **Network Simulation**: Docker-based simulation of 4G/5G cellular conditions
- **Stress Testing**: Multi-stream, high-bitrate, and link failure scenarios  
- **Performance Reporting**: Comprehensive HTML and JSON reports
- **Container-based Testing**: Isolated sender, receiver, and network simulation

## Network Simulation Profiles

The test infrastructure simulates four different network conditions:

1. **Good 4G**: 50ms latency, 1% loss, 20Mbps bandwidth
2. **Poor 4G**: 150ms latency, 5% loss, 10Mbps bandwidth  
3. **5G**: 20ms latency, 0.1% loss, 100Mbps bandwidth
4. **Variable**: Dynamic conditions that change over time

## Quick Start

### Prerequisites

- Rust 1.75+
- GStreamer 1.20+
- Docker and Docker Compose (for full testing)
- Python 3.8+ with matplotlib, numpy

### Build and Test

```bash
# Build the plugin
cargo build --release

# Run basic functionality tests
./scripts/build-test-env.sh
./scripts/test-basic.sh

# Run network simulation tests (requires Docker)
./scripts/test-network-sim.sh

# Run stress tests
./scripts/test-stress.sh

# Generate performance report
./scripts/generate-report.sh
```

### Docker-based Testing

```bash
# Build and run full integration test
cd docker
docker-compose up -d

# Monitor test progress
docker logs -f rist-sender
docker logs -f rist-receiver

# Check network simulation APIs
curl http://localhost:8091/status  # Good 4G network
curl http://localhost:8092/status  # Poor 4G network
curl http://localhost:8093/status  # 5G network
curl http://localhost:8094/status  # Variable network

# Cleanup
docker-compose down
```

## Test Categories

### Basic Tests
- Plugin registration and element creation
- Property configuration and validation
- Basic pipeline functionality

### Network Simulation Tests
- Multi-link bonding performance
- Adaptive weight adjustment
- Dynamic bitrate control
- Network condition response

### Stress Tests
- High bitrate streaming (10Mbps+)
- Multiple simultaneous streams
- Link failure and recovery
- Resource usage under load

## CI/CD Pipeline

The GitHub Actions workflow (``.github/workflows/ci.yml`) automatically:

1. **Build**: Compiles the plugin with all dependencies
2. **Test**: Runs comprehensive test suites
3. **Simulate**: Executes network simulation scenarios
4. **Report**: Generates performance reports and artifacts
5. **Archive**: Stores test results and logs

### Workflow Triggers

- Push to `main` or `develop` branches
- Pull requests to `main`
- Daily scheduled runs (06:00 UTC)

## Performance Metrics

The test infrastructure measures:

- **Throughput**: Total and per-link bandwidth utilization
- **Latency**: Round-trip time measurements across links
- **Packet Loss**: Loss rates and recovery statistics
- **Bonding Efficiency**: Load balancing effectiveness
- **Resource Usage**: CPU and memory consumption
- **Adaptive Response**: Bitrate and weight adjustment speed

## Network Control API

Each network simulation container exposes a REST API:

```bash
# Get current network status
GET /status

# Update network conditions
POST /update
{
    "latency_ms": 100,
    "loss_pct": 2.5,
    "bandwidth_mbps": 15,
    "jitter_ms": 8
}

# Apply network preset
GET /preset/poor-4g

# Get interface statistics  
GET /stats
```

## Test Results

Test results are organized in the `test-results/` directory:

```
test-results/
├── basic/              # Basic functionality tests
├── network-sim/        # Network simulation results
├── stress/             # Stress test results
├── docker-integration/ # Container integration tests
└── logs/               # Detailed logs and traces
```

Reports are generated in the `reports/` directory:

```
reports/
├── performance-report.html    # Comprehensive HTML report
├── performance-report.json    # Machine-readable results
├── test-summary.txt          # Quick summary
└── *.png                     # Performance charts
```

## Customization

### Adding New Network Profiles

Edit `scripts/generate-test-content.py` to add new network profiles:

```python
"custom-network": {
    "latency_ms": 80,
    "jitter_ms": 15, 
    "loss_percent": 3.0,
    "bandwidth_mbps": 25,
    "description": "Custom network profile"
}
```

### Creating Custom Tests

Add new test scripts in the appropriate `scripts/` subdirectory and update the workflow files.

### Modifying Simulation Parameters

Update the Docker Compose environment variables or network control API calls to adjust simulation parameters.

## Troubleshooting

### Common Issues

1. **Plugin not found**: Ensure `GST_PLUGIN_PATH` is set correctly
2. **Docker permissions**: Add user to docker group or run with sudo
3. **Network simulation not working**: Requires privileged containers for tc
4. **Python dependencies**: Install with `pip3 install matplotlib numpy flask`

### Debug Logging

Enable detailed GStreamer logging:

```bash
export GST_DEBUG=ristdispatcher:6,dynbitrate:5,ristsink:5
```

### Container Debugging

```bash
# Enter running container
docker exec -it rist-sender bash

# Check container logs
docker logs --tail 100 rist-network-1

# Monitor network traffic
docker exec rist-network-1 tcpdump -i any
```

## Contributing

When adding new tests:

1. Follow the existing naming conventions
2. Add appropriate error handling and logging
3. Update the CI/CD workflow if needed
4. Document any new dependencies or requirements
5. Test both Docker and local execution paths