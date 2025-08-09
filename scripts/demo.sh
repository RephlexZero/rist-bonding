#!/bin/bash

# RIST Bonding Plugin Demo Script
set -e

echo "=========================================="
echo "RIST Bonding Plugin CI/CD Demo"
echo "=========================================="
echo

# Set up environment
export GST_PLUGIN_PATH="$(pwd)/target/release"

echo "1. Plugin Information"
echo "--------------------"
echo "Plugin Version: $(grep '^version' Cargo.toml | cut -d'"' -f2)"
echo "GStreamer Plugin Path: $GST_PLUGIN_PATH"
echo "Plugin Elements:"
gst-inspect-1.0 ristsmart | grep -E "ristdispatcher|dynbitrate"
echo

echo "2. Running Basic Tests"
echo "---------------------"
./scripts/test-basic.sh | grep -E "✓|Test [0-9]"
echo

echo "3. Testing Multi-Stream Capability"  
echo "--------------------------------"
python3 scripts/test-multiple-streams.py | grep -E "Testing|Results|Total|Passed|Failed|Success"
echo

echo "4. Demonstrating Network Bonding Concepts"
echo "----------------------------------------"
echo "The RIST bonding plugin provides:"
echo "  • ristdispatcher: Intelligent load balancing across 4G/5G links"
echo "  • dynbitrate: Adaptive bitrate control based on network conditions"
echo "  • Automatic weight adjustment using EWMA/AIMD strategies"
echo "  • Real-time statistics and performance monitoring"
echo

echo "5. Network Simulation Capabilities"
echo "--------------------------------"
echo "Test infrastructure simulates:"
echo "  • Good 4G: 50ms latency, 1% loss, 20Mbps"
echo "  • Poor 4G: 150ms latency, 5% loss, 10Mbps"
echo "  • 5G: 20ms latency, 0.1% loss, 100Mbps"
echo "  • Variable: Dynamic conditions changing over time"
echo

echo "6. Generated Test Artifacts"
echo "-------------------------"
if [[ -d "reports" ]]; then
    echo "Performance Reports:"
    ls -la reports/ | grep -E "\.html|\.json|\.png"
fi

if [[ -d "test-results" ]]; then
    echo "Test Results:"
    find test-results -name "*.txt" -o -name "*.json" | head -5
fi
echo

echo "7. CI/CD Workflow Features"
echo "------------------------"
echo "GitHub Actions workflow includes:"
echo "  • Automated build and test on push/PR"
echo "  • Docker-based network simulation"
echo "  • Stress testing with high bitrates and multiple streams"
echo "  • Performance report generation with charts"
echo "  • Daily scheduled testing for continuous monitoring"
echo "  • Artifact collection for analysis"
echo

echo "8. Sample Performance Metrics"
echo "----------------------------"
if [[ -f "reports/performance-report.json" ]]; then
    echo "Latest test results:"
    python3 -c "
import json
with open('reports/performance-report.json') as f:
    data = json.load(f)
print(f'  Plugin loaded: {data.get(\"plugin_info\", {}).get(\"loaded\", False)}')
print(f'  Element creation: {\"PASSED\" if data.get(\"basic_tests\", {}).get(\"element_creation\", False) else \"FAILED\"}')
print(f'  Property tests: {data.get(\"basic_tests\", {}).get(\"property_tests\", {}).get(\"passed\", 0)} passed')
print(f'  Test timestamp: {data.get(\"timestamp\", \"unknown\")}')
"
else
    echo "  Run './scripts/generate-report.sh' to generate performance metrics"
fi
echo

echo "=========================================="
echo "Demo completed successfully!"
echo "=========================================="
echo "To run the full test suite:"
echo "  1. ./scripts/build-test-env.sh"
echo "  2. ./scripts/test-basic.sh"
echo "  3. ./scripts/test-stress.sh"
echo "  4. ./scripts/generate-report.sh"
echo
echo "To run with Docker simulation:"
echo "  cd docker && docker-compose up -d"
echo
echo "View results: open reports/performance-report.html"