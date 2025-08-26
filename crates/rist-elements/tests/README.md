# Test Organization for rist-elements

This document describes the new organized test structure for the rist-elements crate.

## Overview

The integration_tests crate has been **removed** and its functionality merged into rist-elements to:
- Eliminate circular dependencies
- Reduce redundancy and maintenance overhead
- Simplify the workspace structure
- Organize tests by type for better maintainability

## New Test Structure

Tests are now organized in subdirectories by type:

### Unit Tests (`unit/`)
Pure unit tests that don't require external dependencies like GStreamer pipelines or networking:
- `unit/swrr_algorithm.rs` - SWRR algorithm logic tests
- `unit/ewma_algorithm.rs` - EWMA algorithm tests

### Integration Tests (`integration/`)
Tests that involve GStreamer pipelines, elements, and cross-component interactions:
- `integration/pipeline_tests.rs` - GStreamer pipeline validation
- `integration/network_integration.rs` - Network namespace integration tests
- `integration/element_integration.rs` - Element behavior tests

### Stress Tests (`stress/`)
Performance and load testing:
- `stress/pad_lifecycle.rs` - Pad creation/deletion stress tests
- `stress/stats_polling.rs` - Statistics collection stress tests

### Scenario Tests (`scenarios/`)
Specific use-case and workflow testing:
- `scenarios/recovery_scenarios.rs` - Recovery behavior tests
- `scenarios/weighted_flow.rs` - Weighted flow distribution tests
- `scenarios/rist_integration.rs` - RIST protocol integration tests
- `scenarios/stats_driven_rebalancing.rs` - Statistics-driven rebalancing tests

## Running Tests

### All tests
```bash
cargo test -p rist-elements
```

### By category
```bash
# Unit tests only
cargo test -p rist-elements --test unit_tests

# Integration tests only  
cargo test -p rist-elements --test integration_tests

# Stress tests only
cargo test -p rist-elements --test stress_tests

# Scenario tests only
cargo test -p rist-elements --test scenario_tests
```

### Network namespace tests (requires root)
```bash
# Using the wrapper script (recommended)
./scripts/run_automated_integration_sudo.sh

# Direct (be careful about root-owned files)
sudo -E cargo test -p rist-elements --test integration_tests -- --nocapture
```

### Individual test in a category
```bash
cargo test -p rist-elements --test unit_tests unit::swrr_algorithm::test_basic_property_access
```

## Directory Structure

```
tests/
├── README.md
├── unit_tests.rs          # Entry point for unit tests
├── integration_tests.rs   # Entry point for integration tests  
├── stress_tests.rs        # Entry point for stress tests
├── scenario_tests.rs      # Entry point for scenario tests
├── unit/
│   ├── mod.rs
│   ├── swrr_algorithm.rs
│   └── ewma_algorithm.rs
├── integration/
│   ├── mod.rs
│   ├── pipeline_tests.rs
│   ├── network_integration.rs
│   └── element_integration.rs
├── stress/
│   ├── mod.rs
│   ├── pad_lifecycle.rs
│   └── stats_polling.rs
└── scenarios/
    ├── mod.rs
    ├── recovery_scenarios.rs
    ├── weighted_flow.rs
    ├── rist_integration.rs
    └── stats_driven_rebalancing.rs
```

## Dependencies

The test dependencies are now included directly in rist-elements' `dev-dependencies`:
- `anyhow` - Error handling
- `chrono` - Time handling with serde support
- `serial_test` - Sequential test execution
- `serde_json` - JSON serialization for test artifacts
- `tracing-subscriber` - Logging support

Network-related dependencies are gated behind the `netns-sim` feature (enabled by default):
- `netns-testbench` - Network namespace orchestration
- `scenarios` - Test scenario definitions
- `tokio` - Async runtime

## Artifacts

Test artifacts are written to:
1. `TEST_ARTIFACTS_DIR` environment variable, if set
2. `CARGO_TARGET_DIR/test-artifacts`, if `CARGO_TARGET_DIR` is set  
3. `<workspace>/target/test-artifacts` (default)

## Migration Notes

- The old `integration_tests` crate has been completely removed
- All functionality has been moved into `rist-elements/tests/` with proper directory organization
- Tests are now organized in subdirectories by type, making them easier to find and maintain
- Scripts and documentation updated to reference the new test structure