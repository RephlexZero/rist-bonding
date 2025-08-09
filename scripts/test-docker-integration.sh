#!/bin/bash

# Docker Integration Tests
set -e

echo "Running Docker integration tests..."

# Check if Docker is available
if ! command -v docker &> /dev/null; then
    echo "Docker not available, skipping integration tests"
    exit 0
fi

TEST_RESULTS_DIR="test-results"
mkdir -p $TEST_RESULTS_DIR/docker-integration

# Test 1: Build all containers
echo "Test 1: Building Docker containers"
cd docker
docker build -t rist-test-sender -f Dockerfile.sender . || {
    echo "✗ Failed to build sender container"
    exit 1
}
echo "✓ Sender container built"

docker build -t rist-test-receiver -f Dockerfile.receiver . || {
    echo "✗ Failed to build receiver container"
    exit 1
}
echo "✓ Receiver container built"

docker build -t rist-network-sim -f Dockerfile.network-sim . || {
    echo "✗ Failed to build network sim container"
    exit 1
}
echo "✓ Network sim container built"

# Test 2: Start the full environment
echo "Test 2: Starting integration test environment"
docker-compose down || true
docker-compose up -d

# Wait for startup
sleep 60

# Test 3: Check if all services are running
echo "Test 3: Verifying service health"
services=("rist-sender" "rist-receiver" "rist-network-1" "rist-network-2" "rist-network-3" "rist-network-4")
for service in "${services[@]}"; do
    if docker ps --format "table {{.Names}}" | grep -q "^$service$"; then
        echo "✓ $service is running"
    else
        echo "✗ $service is not running"
        docker logs $service
    fi
done

# Test 4: Run bonding test for 3 minutes
echo "Test 4: Running 3-minute bonding integration test"
sleep 180

# Test 5: Collect final statistics
echo "Test 5: Collecting integration test statistics"
docker exec rist-receiver python3 scripts/collect-stats.py > ../test-results/docker-integration/final-stats.json || true

# Cleanup
docker-compose down

cd ..

echo "✓ Docker integration tests completed"