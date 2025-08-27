# RIST Bonding Development Container

This directory contains the VS Code devcontainer configuration for the RIST Bonding project. The devcontainer provides a complete development environment with all necessary tools and dependencies pre-installed.

## Features

- **Complete Rust toolchain** with rust-analyzer and debugging support
- **GStreamer development environment** with all plugins and dependencies
- **Network simulation capabilities** with network namespaces and traffic control
- **All project dependencies** pre-installed and configured
- **VS Code extensions** for Rust development, debugging, and testing
- **Docker-in-Docker support** for container-based testing

## Quick Start

1. **Prerequisites**:
   - VS Code with the Remote-Containers extension
   - Docker and Docker Compose installed on your host

2. **Open in Container**:
   ```bash
   # Option 1: From command line
   code --folder-uri vscode-remote://dev-container+/path/to/rist-bonding
   
   # Option 2: From VS Code
   # - Open the rist-bonding folder in VS Code
   # - Press F1 and select "Remote-Containers: Reopen in Container"
   ```

3. **Start Development**:
   - The container will automatically build on first open
   - All dependencies are pre-installed
   - Network capabilities are enabled for testing

## Available Commands

The devcontainer includes several pre-configured tasks accessible via `Ctrl+Shift+P` → "Tasks: Run Task":

- **cargo: build all** - Build all crates with all features
- **cargo: test all** - Run all tests
- **cargo: clippy** - Run linter with strict warnings
- **Docker: Run All Tests** - Execute the complete test suite in Docker
- **Docker: Setup Network Test Environment** - Set up network namespaces for testing
- **Docker: Interactive Development Shell** - Open a shell in the test environment

## Development Workflow

1. **Code Editing**:
   - Rust-analyzer provides intelligent code completion and error detection
   - Error Lens shows inline diagnostics
   - TOML support for Cargo.toml files

2. **Testing**:
   ```bash
   # Run specific test suites
   cargo test -p rist-elements
   cargo test -p network-sim --features docker
   
   # Or use the integrated Docker test script
   ./scripts/docker-test.sh test
   ```

3. **Debugging**:
   - Press F5 to debug tests
   - Set breakpoints in your code
   - Use the integrated terminal for interactive debugging

4. **Network Testing**:
   ```bash
   # Set up network test environment
   ./scripts/docker-test.sh network
   
   # Run network simulation tests
   cargo test -p network-sim --features docker
   ```

## Container Configuration

The devcontainer is based on our existing Docker development image and includes:

- **Base**: Ubuntu 22.04 with development tools
- **Rust**: Latest stable toolchain with clippy and rustfmt
- **GStreamer**: Complete development stack with RIST plugin support
- **Network Tools**: iproute2, tcpdump, iperf3 for network simulation
- **Capabilities**: NET_ADMIN and SYS_ADMIN for network namespace operations

## Port Forwarding

The following ports are automatically forwarded from the container:

- `1968-1971`: RIST transport ports
- `8080-8081`: HTTP servers for documentation and monitoring

## Troubleshooting

### Container Build Issues
```bash
# Rebuild the container
docker-compose build rist-bonding-dev --no-cache
```

### Network Permissions
```bash
# Verify network capabilities
./scripts/docker-test.sh network
```

### Rust Analyzer Issues
```bash
# Restart rust-analyzer
# In VS Code: Ctrl+Shift+P → "Rust Analyzer: Restart Server"
```

### Performance Optimization
- The container mounts the project directory as a volume
- Target directories are excluded from file watching for better performance
- Docker layer caching speeds up subsequent builds

## Advanced Usage

### Custom Development Tasks
Add custom tasks to `.devcontainer/devcontainer.json` or the workspace configuration.

### Additional Extensions
The devcontainer automatically installs recommended extensions, but you can add more in the `customizations.vscode.extensions` section.

### Environment Variables
Customize the development environment by modifying `containerEnv` in the devcontainer configuration.

## Integration with CI/CD

The devcontainer uses the same Docker configuration as our CI/CD pipeline, ensuring consistency between development and production environments.