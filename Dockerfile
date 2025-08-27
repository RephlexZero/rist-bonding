# Multi-stage Docker build for RIST bonding with network simulation
FROM ubuntu:22.04 as base

# Avoid interactive prompts during package installation
ENV DEBIAN_FRONTEND=noninteractive

# Install system dependencies
RUN apt-get update && apt-get install -y \
    curl \
    build-essential \
    pkg-config \
    libssl-dev \
    libgstreamer1.0-dev \
    libgstreamer-plugins-base1.0-dev \
    libgstreamer-plugins-bad1.0-dev \
    gstreamer1.0-plugins-base \
    gstreamer1.0-plugins-good \
    gstreamer1.0-plugins-bad \
    gstreamer1.0-plugins-ugly \
    gstreamer1.0-libav \
    libfontconfig1-dev \
    libfreetype6-dev \
    iproute2 \
    net-tools \
    iputils-ping \
    tcpdump \
    iperf3 \
    git \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# Set working directory
WORKDIR /workspace

# Stage for development and testing
FROM base as development

# Copy the entire project
COPY . .

# Create network setup script for all containers
RUN cat > /usr/local/bin/setup-network-test.sh << 'EOF'
#!/bin/bash
set -euo pipefail

echo "Setting up network namespaces for testing..."

# Create network namespaces
ip netns add ns1 2>/dev/null || true
ip netns add ns2 2>/dev/null || true

# Create veth pairs
ip link add veth0 type veth peer name veth1 2>/dev/null || true
ip link add veth2 type veth peer name veth3 2>/dev/null || true

# Move interfaces to namespaces
ip link set veth1 netns ns1 2>/dev/null || true
ip link set veth3 netns ns2 2>/dev/null || true

# Configure interfaces in root namespace
ip addr add 10.0.1.1/24 dev veth0 2>/dev/null || true
ip addr add 10.0.2.1/24 dev veth2 2>/dev/null || true
ip link set veth0 up 2>/dev/null || true
ip link set veth2 up 2>/dev/null || true

# Configure interfaces in namespaces
ip netns exec ns1 ip addr add 10.0.1.2/24 dev veth1 2>/dev/null || true
ip netns exec ns1 ip link set veth1 up 2>/dev/null || true
ip netns exec ns1 ip link set lo up 2>/dev/null || true

ip netns exec ns2 ip addr add 10.0.2.2/24 dev veth3 2>/dev/null || true
ip netns exec ns2 ip link set veth3 up 2>/dev/null || true
ip netns exec ns2 ip link set lo up 2>/dev/null || true

# Add routing
ip netns exec ns1 ip route add default via 10.0.1.1 2>/dev/null || true
ip netns exec ns2 ip route add default via 10.0.2.1 2>/dev/null || true

echo "Network setup complete!"
echo "Available interfaces:"
echo "  Root namespace: veth0 (10.0.1.1), veth2 (10.0.2.1)"
echo "  Namespace ns1: veth1 (10.0.1.2)"
echo "  Namespace ns2: veth3 (10.0.2.2)"

# Test connectivity
echo "Testing connectivity..."
ping -c 1 -W 1 10.0.1.2 > /dev/null && echo "✓ veth0 -> ns1 connectivity OK" || echo "⚠ veth0 -> ns1 connectivity failed"
ping -c 1 -W 1 10.0.2.2 > /dev/null && echo "✓ veth2 -> ns2 connectivity OK" || echo "⚠ veth2 -> ns2 connectivity failed"
EOF

RUN chmod +x /usr/local/bin/setup-network-test.sh

# Build the project
RUN cargo build --release

# Set up network capabilities for testing
# Note: This will require running the container with --cap-add=NET_ADMIN
LABEL network.capabilities="NET_ADMIN"

# Default command for development
CMD ["bash"]

# Stage for testing with network simulation
FROM development as testing

# Install additional testing tools
RUN apt-get update && apt-get install -y \
    stress-ng \
    netcat-openbsd \
    socat \
    && rm -rf /var/lib/apt/lists/*

# Create a script to set up network namespaces for testing
RUN cat > /usr/local/bin/setup-network-test.sh << 'EOF'
#!/bin/bash
set -euo pipefail

echo "Setting up network namespaces for testing..."

# Create network namespaces
ip netns add ns1 2>/dev/null || true
ip netns add ns2 2>/dev/null || true

# Create veth pairs
ip link add veth0 type veth peer name veth1 2>/dev/null || true
ip link add veth2 type veth peer name veth3 2>/dev/null || true

# Move interfaces to namespaces
ip link set veth1 netns ns1 2>/dev/null || true
ip link set veth3 netns ns2 2>/dev/null || true

# Configure interfaces in root namespace
ip addr add 10.0.1.1/24 dev veth0 2>/dev/null || true
ip addr add 10.0.2.1/24 dev veth2 2>/dev/null || true
ip link set veth0 up 2>/dev/null || true
ip link set veth2 up 2>/dev/null || true

# Configure interfaces in namespaces
ip netns exec ns1 ip addr add 10.0.1.2/24 dev veth1 2>/dev/null || true
ip netns exec ns1 ip link set veth1 up 2>/dev/null || true
ip netns exec ns1 ip link set lo up 2>/dev/null || true

ip netns exec ns2 ip addr add 10.0.2.2/24 dev veth3 2>/dev/null || true
ip netns exec ns2 ip link set veth3 up 2>/dev/null || true
ip netns exec ns2 ip link set lo up 2>/dev/null || true

# Add routing
ip netns exec ns1 ip route add default via 10.0.1.1 2>/dev/null || true
ip netns exec ns2 ip route add default via 10.0.2.1 2>/dev/null || true

echo "Network setup complete!"
echo "Available interfaces:"
echo "  Root namespace: veth0 (10.0.1.1), veth2 (10.0.2.1)"
echo "  Namespace ns1: veth1 (10.0.1.2)"
echo "  Namespace ns2: veth3 (10.0.2.2)"

# Test connectivity
echo "Testing connectivity..."
ping -c 1 -W 1 10.0.1.2 > /dev/null && echo "✓ veth0 -> ns1 connectivity OK"
ping -c 1 -W 1 10.0.2.2 > /dev/null && echo "✓ veth2 -> ns2 connectivity OK"
EOF

RUN chmod +x /usr/local/bin/setup-network-test.sh

# Create a script to run tests with network simulation
RUN cat > /usr/local/bin/run-network-tests.sh << 'EOF'
#!/bin/bash
set -euo pipefail

echo "Running network simulation tests..."

# Set up network namespaces
/usr/local/bin/setup-network-test.sh

# Run the Rust tests
echo "Running Rust tests..."
cd /workspace

# Run unit tests first
echo "=== Running unit tests ==="
cargo test --lib

# Run integration tests with network features enabled
echo "=== Running integration tests ==="
cargo test --test integration_tests --features network-sim

# Run scenario tests
echo "=== Running scenario tests ==="
cargo test --test scenario_tests

# Run stress tests
echo "=== Running stress tests ==="
cargo test --test stress_tests

# Run network-specific integration tests
echo "=== Running network integration tests ==="
cargo test -p network-sim

echo "All tests completed!"
EOF

RUN chmod +x /usr/local/bin/run-network-tests.sh

# Default command for testing
CMD ["/usr/local/bin/run-network-tests.sh"]