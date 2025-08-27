# Docker-Based Testing Guide

This document explains how to run RIST bonding tests using Docker containers, which eliminates the need for sudo privileges on the host system and provides a consistent testing environment with network namespace capabilities.

## Quick Start

### Prerequisites

- Docker and Docker Compose installed
- At least 2GB of free disk space for Docker images

### Running All Tests

```bash
# Run the complete test suite in Docker
./scripts/docker-test.sh test
```

### Interactive Development

```bash
# Start an interactive development container
./scripts/docker-test.sh dev
```

## Docker Architecture

### Container Setup

The Docker setup provides three main container configurations:

1. **Development Container** (`rist-bonding-dev`)
   - Full development environment with Rust toolchain
   - Network capabilities for namespace testing
   - Interactive shell access

2. **Testing Container** (`rist-bonding-test`)
   - Automated test runner
   - Pre-configured network namespaces
   - Runs complete test suite

3. **Multi-Node Container** (`rist-bonding-node2`)
   - Second container for multi-node testing
   - Network connectivity testing between containers

### Network Capabilities

The containers are configured with:
- `NET_ADMIN` capability for network namespace management
- `SYS_ADMIN` capability for advanced system operations
- Privileged mode for full network control
- Custom bridge network for container-to-container communication

## Available Commands

### Testing Commands

```bash
# Run all tests in Docker
./scripts/docker-test.sh test

# Run specific test
./scripts/docker-test.sh run test_weighted_distribution_basic

# Run benchmarks
./scripts/docker-test.sh bench

# Run multi-container network tests
./scripts/docker-test.sh multi
```

### Development Commands

```bash
# Start interactive development container
./scripts/docker-test.sh dev

# Build Docker images
./scripts/docker-test.sh build

# Show network status inside container
./scripts/docker-test.sh network

# Clean up Docker resources
./scripts/docker-test.sh clean
```

### Inside the Container

Once in the development container, you can:

```bash
# Set up network namespaces manually
/usr/local/bin/setup-network-test.sh

# Run tests with network features
cargo test --features docker

# Run network simulation tests
cargo test -p network-sim --features docker

# Apply network impairments to test interfaces
cargo test test_network_impairment_scenarios

# Monitor network interfaces
ip addr show
ip netns list
```

## Network Simulation Features

### Namespace Management

The Docker environment provides:
- Automatic creation of test network namespaces (`test_ns1`, `test_ns2`)
- Virtual ethernet pairs (`veth_test0`, `veth_test1`, etc.)
- IP address configuration and routing
- Interface management within namespaces

### Traffic Control

Network impairments can be applied using:
- **Latency**: Configurable delay in milliseconds
- **Packet Loss**: Configurable loss percentage
- **Bandwidth Limiting**: Rate limiting in kbps
- **Jitter**: Network timing variations

### Example Usage

```rust
use network_sim::docker::DockerNetworkEnv;

#[tokio::test]
async fn test_network_impairments() {
    let mut env = DockerNetworkEnv::new();
    
    // Set up test network
    env.setup_basic_network().await?;
    
    // Apply network impairments: 50ms delay, 1% loss, 1Mbps limit
    env.apply_network_impairments("veth_test0", 50, 1.0, 1000).await?;
    
    // Test connectivity
    let connected = env.test_connectivity("test_ns1", "192.168.100.1").await?;
    assert!(connected);
    
    // Cleanup
    env.cleanup().await?;
}
```

## Troubleshooting

### Permission Errors

If you see "Operation not permitted" errors:

```bash
# Make sure Docker is running with proper privileges
sudo systemctl start docker

# Verify the script is executable
chmod +x ./scripts/docker-test.sh

# Check Docker can run privileged containers
docker run --rm --privileged alpine ip addr show
```

### Network Issues

If network tests fail:

```bash
# Check container network capabilities
./scripts/docker-test.sh network

# Verify network namespaces are created
docker-compose run --rm rist-bonding-dev ip netns list

# Test basic connectivity
docker-compose run --rm rist-bonding-dev ping -c 1 127.0.0.1
```

### Build Issues

If Docker builds fail:

```bash
# Clean up and rebuild
./scripts/docker-test.sh clean
./scripts/docker-test.sh build

# Check Docker system resources
docker system df
docker system prune -f
```

### Container Resource Issues

If containers run out of resources:

```bash
# Check container resource usage
docker stats

# Clean up unused containers and images
docker system prune -a -f

# Increase Docker memory limit in Docker Desktop settings
```

## CI/CD Integration

### GitHub Actions

The project includes GitHub Actions workflows for Docker-based testing:

- **Standard Tests**: Basic functionality without special privileges
- **Docker Tests**: Full network simulation tests with capabilities
- **Multi-Container Tests**: Container-to-container networking
- **Security Checks**: Code quality and vulnerability scanning

### Local CI Testing

To test the CI workflow locally:

```bash
# Install act (GitHub Actions local runner)
curl https://raw.githubusercontent.com/nektos/act/master/install.sh | sudo bash

# Run GitHub Actions locally
act -j test-docker
```

## Performance Considerations

### Docker Overhead

- Network namespace operations have ~1-2ms additional latency
- Container startup adds ~500ms to test execution
- Memory usage: ~200MB per container

### Optimization Tips

- Use Docker layer caching for faster builds
- Pre-build containers for repeated testing
- Use `.dockerignore` to exclude unnecessary files
- Leverage Docker Compose for multi-container orchestration

## Advanced Configuration

### Custom Network Scenarios

Create custom network scenarios by modifying the setup script:

```bash
# Edit the network setup script
vim /usr/local/bin/setup-network-test.sh

# Add custom network configurations
ip netns add custom_ns
ip link add veth_custom type veth peer name veth_custom_peer
```

### Container Networking

Modify `docker-compose.yml` for custom networking:

```yaml
networks:
  custom-network:
    driver: bridge
    ipam:
      config:
        - subnet: 10.100.0.0/16
          gateway: 10.100.0.1
```

### Environment Variables

Configure container behavior with environment variables:

```bash
# Set in docker-compose.yml or pass to docker run
RUST_LOG=debug
NETWORK_TEST_INTERFACE=eth0
TEST_TIMEOUT=60
```

## Best Practices

1. **Always clean up** after tests to avoid resource leaks
2. **Use specific test interfaces** to avoid conflicts
3. **Check capabilities** before running privileged operations
4. **Monitor resource usage** during long test runs
5. **Use Docker layer caching** for faster rebuilds
6. **Test locally** before pushing to CI
7. **Document custom configurations** in your test code

## Integration with Host System

### File System Access

The containers mount the project directory at `/workspace`:
- Changes made in containers persist on the host
- Build artifacts are shared between host and container
- Source code changes are immediately available

### Network Isolation

Containers use separate network namespaces:
- Tests don't interfere with host networking
- Multiple test runs can execute simultaneously
- Network configurations are completely isolated

This Docker-based approach provides a robust, reproducible testing environment that doesn't require sudo privileges on the host system while still enabling comprehensive network simulation testing.