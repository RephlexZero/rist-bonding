#!/bin/bash

# Network Simulation Tests
set -e

echo "Running network simulation tests..."

export GST_PLUGIN_PATH="$(pwd)/target/release"
TEST_RESULTS_DIR="test-results"
mkdir -p $TEST_RESULTS_DIR/{network-sim,logs}

# Function to check if docker is available
check_docker() {
    if ! command -v docker &> /dev/null; then
        echo "Docker not available, running limited network tests"
        return 1
    fi
    return 0
}

# Function to run local network tests without docker
run_local_tests() {
    echo "Running local network simulation tests..."
    
    # Create a simple loopback test
    echo "Test: Local loopback with simulated conditions"
    
    # Use tc (traffic control) if available to simulate network conditions
    if command -v tc &> /dev/null && [[ $EUID -eq 0 ]]; then
        echo "Running network simulation with traffic control..."
        python3 scripts/run-local-network-test.py
    else
        echo "Running basic loopback test without network simulation..."
        python3 scripts/run-basic-loopback-test.py
    fi
}

# Function to run docker-based network tests
run_docker_tests() {
    echo "Starting Docker-based network simulation tests..."
    
    cd docker
    
    # Start the test environment
    echo "Starting Docker Compose environment..."
    docker-compose up -d
    
    # Wait for services to be ready
    echo "Waiting for services to start..."
    sleep 30
    
    # Check service health
    for service in rist-sender rist-receiver rist-network-1 rist-network-2 rist-network-3 rist-network-4; do
        if docker ps | grep -q $service; then
            echo "✓ $service is running"
        else
            echo "✗ $service failed to start"
            docker-compose logs $service
        fi
    done
    
    # Run tests for 2 minutes
    echo "Running bonding tests for 120 seconds..."
    sleep 120
    
    # Collect statistics
    echo "Collecting test statistics..."
    python3 ../scripts/collect-network-stats.py
    
    # Stop the environment
    docker-compose down
    
    cd ..
}

# Main test execution
if check_docker; then
    run_docker_tests
else
    run_local_tests
fi

echo "✓ Network simulation tests completed"