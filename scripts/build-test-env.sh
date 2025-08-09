#!/bin/bash

# Build Test Environment Script
set -e

echo "Building RIST Bonding Test Environment..."

# Create necessary directories
mkdir -p test-results/{logs,stats,reports}
mkdir -p test-data

# Verify plugin is available
if [[ -f "target/release/libgstristsmart.so" ]]; then
    echo "✓ RIST Smart plugin found"
    export GST_PLUGIN_PATH="$(pwd)/target/release"
else
    echo "✗ RIST Smart plugin not found. Please build first with 'cargo build --release'"
    exit 1
fi

# Test plugin registration
echo "Testing plugin registration..."
gst-inspect-1.0 ristsmart > /dev/null 2>&1 && echo "✓ Plugin registered successfully" || {
    echo "✗ Plugin registration failed"
    exit 1
}

# Generate test video content
echo "Generating test video content..."
python3 scripts/generate-test-content.py

# Start network simulation containers if docker is available
if command -v docker &> /dev/null && command -v docker-compose &> /dev/null; then
    echo "Setting up Docker test environment..."
    
    # Stop any existing containers
    docker-compose -f docker/docker-compose.yml down || true
    
    echo "✓ Test environment ready"
else
    echo "⚠ Docker not available, skipping container setup"
fi

echo "✓ Build test environment completed"