# RIST Bonding Development Container

This directory contains the VS Code devcontainer configuration for the RIST Bonding project. The devcontainer provides a complete development environment with all necessary tools and dependencies pre-installed.

## Features

- **Complete Rust toolchain** with rust-analyzer and debugging support
- **GStreamer development environment** with all plugins and dependencies
- **Network simulation capabilities** with network namespaces and traffic control
- **All project dependencies** pre-installed and configured
- **VS Code extensions** for Rust development, debugging, and testing

## Quick Start

1. **Prerequisites**:
   - VS Code with the Dev Containers extension
   - Docker installed on your host

2. **Open in Container**:
   ```bash
   # Open the project folder in VS Code
   code .
   # Select "Reopen in Container" when prompted
   ```

3. **Start Development**:
   - The container will automatically build on first open
   - All dependencies are pre-installed
   - Network capabilities are enabled for testing

## Development Workflow

1. **Code Editing**:
   - Rust-analyzer provides intelligent code completion and error detection
   - Error Lens shows inline diagnostics
   - TOML support for Cargo.toml files

2. **Testing**:
   ```bash
   # Run all tests
   cargo test --all-features
   
   # Run specific test suites
   cargo test -p rist-elements
   cargo test -p network-sim
   ```

3. **Building**:
   ```bash
   # Build all crates
   cargo build --all-features
   
   # Run code quality checks
   cargo fmt --all
   cargo clippy --all-targets --all-features -- -D warnings
   ```

4. **Network Testing**:
   ```bash
   # Run network simulation tests (automatic setup/cleanup)
   cargo test -p network-sim --all-features
   
   # Run RIST integration tests (automatic namespace management)  
   cargo test -p rist-elements --test integration_tests --all-features
   ```

## Container Configuration

The devcontainer is based on Ubuntu 22.04 and includes:

- **Rust**: Latest stable toolchain with clippy and rustfmt
- **GStreamer**: Complete development stack with RIST plugin support
- **Network Tools**: iproute2, tcpdump, and other networking utilities
- **Capabilities**: NET_ADMIN and SYS_ADMIN for network namespace operations

## Port Forwarding

The following ports are automatically forwarded from the container:

- `1968-1971`: RIST transport ports for testing
- `8080-8081`: HTTP servers for documentation and monitoring

## Troubleshooting

### Container Build Issues
```bash
# Rebuild the container
Ctrl+Shift+P → "Dev Containers: Rebuild Container"
```

### Network Permissions
```bash
# Verify network capabilities are available
ip netns add test-ns && ip netns del test-ns
```

### Rust Analyzer Issues
```bash
# Restart rust-analyzer
# In VS Code: Ctrl+Shift+P → "Rust Analyzer: Restart Server"
```

The devcontainer provides a consistent development environment that matches the CI/CD pipeline, ensuring your local development experience aligns with automated testing.