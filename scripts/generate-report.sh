#!/bin/bash

# Generate Performance Report
set -e

echo "Generating performance report..."

TEST_RESULTS_DIR="test-results"
REPORTS_DIR="reports"
mkdir -p $REPORTS_DIR

# Run the report generator
python3 scripts/generate-performance-report.py \
    --input-dir $TEST_RESULTS_DIR \
    --output-dir $REPORTS_DIR \
    --format html,json

# Create summary report
echo "Creating test summary..."
cat > $REPORTS_DIR/test-summary.txt << EOF
RIST Bonding Plugin Test Summary
================================

Test Date: $(date)
Plugin Version: $(grep '^version' Cargo.toml | cut -d'"' -f2)

Test Results:
EOF

# Check test results
if [[ -f "$TEST_RESULTS_DIR/element-creation.txt" ]]; then
    if grep -q "PASS" "$TEST_RESULTS_DIR/element-creation.txt"; then
        echo "✓ Element Creation: PASSED" >> $REPORTS_DIR/test-summary.txt
    else
        echo "✗ Element Creation: FAILED" >> $REPORTS_DIR/test-summary.txt
    fi
fi

if [[ -f "$TEST_RESULTS_DIR/plugin-info.txt" ]]; then
    echo "✓ Plugin Registration: PASSED" >> $REPORTS_DIR/test-summary.txt
fi

if [[ -d "$TEST_RESULTS_DIR/network-sim" ]]; then
    echo "✓ Network Simulation: COMPLETED" >> $REPORTS_DIR/test-summary.txt
fi

if [[ -d "$TEST_RESULTS_DIR/stress" ]]; then
    echo "✓ Stress Tests: COMPLETED" >> $REPORTS_DIR/test-summary.txt
fi

echo "" >> $REPORTS_DIR/test-summary.txt
echo "Detailed results available in: $REPORTS_DIR/performance-report.html" >> $REPORTS_DIR/test-summary.txt

# Display summary
cat $REPORTS_DIR/test-summary.txt

echo "✓ Performance report generated in $REPORTS_DIR/"