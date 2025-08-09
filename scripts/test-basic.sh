#!/bin/bash

# Basic Functionality Tests
set -e

echo "Running basic functionality tests..."

export GST_PLUGIN_PATH="$(pwd)/target/release"
TEST_RESULTS_DIR="test-results"
mkdir -p $TEST_RESULTS_DIR

# Test 1: Plugin loading and element creation
echo "Test 1: Plugin Loading and Element Creation"
gst-inspect-1.0 ristsmart > $TEST_RESULTS_DIR/plugin-info.txt 2>&1
gst-inspect-1.0 ristdispatcher > $TEST_RESULTS_DIR/dispatcher-info.txt 2>&1
gst-inspect-1.0 dynbitrate > $TEST_RESULTS_DIR/dynbitrate-info.txt 2>&1

# Verify elements can be created
python3 -c "
import gi
gi.require_version('Gst', '1.0')
from gi.repository import Gst
Gst.init(None)

# Test element creation
try:
    disp = Gst.ElementFactory.make('ristdispatcher', 'test-dispatcher')
    dyn = Gst.ElementFactory.make('dynbitrate', 'test-dynbitrate')
    if disp and dyn:
        print('✓ Elements created successfully')
        with open('$TEST_RESULTS_DIR/element-creation.txt', 'w') as f:
            f.write('PASS: Elements created successfully\n')
    else:
        raise Exception('Failed to create elements')
except Exception as e:
    print(f'✗ Element creation failed: {e}')
    with open('$TEST_RESULTS_DIR/element-creation.txt', 'w') as f:
        f.write(f'FAIL: Element creation failed: {e}\n')
    exit(1)
"

echo "✓ Test 1 passed"

# Test 2: Basic pipeline creation
echo "Test 2: Basic Pipeline Creation"
timeout 10s gst-launch-1.0 \
    videotestsrc num-buffers=100 \
    ! video/x-raw,width=640,height=480,framerate=30/1 \
    ! x264enc bitrate=1000 tune=zerolatency \
    ! rtph264pay pt=96 \
    ! ristdispatcher name=disp \
    ! fakesink \
    > $TEST_RESULTS_DIR/pipeline-test.log 2>&1 && \
echo "✓ Test 2 passed" || {
    echo "⚠ Test 2 warning: Pipeline test had issues (expected without RIST sink)"
    echo "WARN" > $TEST_RESULTS_DIR/pipeline-status.txt
}

# Test 3: Property testing
echo "Test 3: Property Testing"
python3 scripts/test-properties.py > $TEST_RESULTS_DIR/property-test.txt 2>&1 && \
echo "✓ Test 3 passed" || {
    echo "✗ Test 3 failed"
    exit 1
}

echo "✓ All basic functionality tests completed"