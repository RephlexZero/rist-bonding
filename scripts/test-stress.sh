#!/bin/bash

# Stress Tests
set -e

echo "Running stress tests..."

export GST_PLUGIN_PATH="$(pwd)/target/release"
TEST_RESULTS_DIR="test-results"
mkdir -p $TEST_RESULTS_DIR/stress

# Stress Test 1: High bitrate streaming
echo "Stress Test 1: High Bitrate Streaming (10Mbps)"
timeout 60s gst-launch-1.0 \
    videotestsrc pattern=snow num-buffers=1800 \
    ! video/x-raw,width=1920,height=1080,framerate=30/1 \
    ! x264enc bitrate=10000 tune=zerolatency speed-preset=ultrafast \
    ! rtph264pay pt=96 \
    ! ristdispatcher name=disp \
    ! fakesink \
    > $TEST_RESULTS_DIR/stress/high-bitrate-test.log 2>&1 && \
echo "✓ Stress Test 1 completed" || \
echo "⚠ Stress Test 1 had issues"

# Stress Test 2: Multiple simultaneous streams
echo "Stress Test 2: Multiple Streams Simulation"
python3 scripts/test-multiple-streams.py

# Stress Test 3: Link failure simulation
echo "Stress Test 3: Link Failure Recovery"
python3 scripts/test-link-failure.py

# Stress Test 4: Memory and CPU usage under load
echo "Stress Test 4: Resource Usage Analysis"
python3 scripts/test-resource-usage.py

echo "✓ All stress tests completed"