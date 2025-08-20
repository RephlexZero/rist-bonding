# bench-cli

Command-line tool for running RIST network testbench scenarios.

## Overview

`bench-cli` is a comprehensive command-line interface for executing network simulation scenarios and RIST bonding tests. It provides an easy-to-use interface for running predefined scenarios, custom network conditions, and automated test suites with detailed reporting and metrics collection.

## Features

- **Scenario Execution**: Run predefined network scenarios from the scenarios crate
- **Custom Network Conditions**: Define and execute custom network impairments
- **Network Backend**: Support for the netns-testbench backend
- **Automated Testing**: Batch execution of test suites with reporting
- **Metrics Collection**: Integration with observability for performance monitoring
- **Configuration Management**: JSON-based scenario configuration

## Installation

```bash
# Build the CLI tool
cargo build --bin bench-cli

# Run from the project directory
cargo run --bin bench-cli -- --help
```

## Usage

### Basic Commands

```bash
# List available scenarios
bench-cli list-scenarios

# Run a specific scenario
bench-cli run --scenario mobile_4g --duration 60

# Run with custom network conditions
bench-cli run --latency 100ms --loss 2% --bandwidth 1Mbps

# Execute a test suite
bench-cli test-suite --config ./test_config.json
```

### Advanced Usage

```bash
# Run with detailed metrics collection
bench-cli run --scenario satellite_link \
    --duration 120 \
    --metrics-output ./results.csv \
    --prometheus-port 9090

# Execute custom scenario from file
bench-cli run --config ./custom_scenario.json \
    --backend netns-testbench \
    --verbose

# Batch testing with reporting
bench-cli batch-test \
    --scenarios mobile_4g,mobile_5g,satellite_link \
    --iterations 5 \
    --output-dir ./test_results
```

## Command Reference

### `run` - Execute a scenario
- `--scenario`: Predefined scenario name
- `--config`: JSON configuration file
- `--duration`: Test duration in seconds
- `--backend`: Backend to use (netns-testbench)
- `--metrics-output`: CSV output file for metrics
- `--prometheus-port`: Enable Prometheus metrics server

### `list-scenarios` - Show available scenarios
Lists all predefined scenarios from the scenarios crate with descriptions.

### `test-suite` - Execute automated test suite
- `--config`: Test suite configuration file
- `--output-dir`: Directory for test results
- `--parallel`: Run tests in parallel

### `validate-config` - Validate scenario configuration
- `--config`: Configuration file to validate

## Configuration Format

### Scenario Configuration (JSON)

```json
{
  "name": "custom_test",
  "description": "Custom network scenario",
  "links": [
    {
      "name": "uplink",
      "latency_ms": 50.0,
      "packet_loss_percent": 1.0,
      "bandwidth_kbps": 1000,
      "jitter_ms": 5.0
    }
  ],
  "schedule": [
    {
      "time_s": 30.0,
      "changes": {
        "uplink": {
          "latency_ms": 100.0
        }
      }
    }
  ],
  "observability": {
    "enable_metrics": true,
    "csv_output": true,
    "prometheus_port": 9090
  }
}
```

### Test Suite Configuration

```json
{
  "test_suite": "rist_bonding_validation",
  "scenarios": [
    {
      "name": "mobile_4g",
      "duration_s": 60,
      "iterations": 3
    },
    {
      "name": "satellite_link", 
      "duration_s": 120,
      "iterations": 2
    }
  ],
  "assertions": {
    "max_packet_loss_percent": 5.0,
    "min_throughput_bps": 500000
  },
  "output": {
    "format": "json",
    "directory": "./results"
  }
}
```

## Example Workflows

### Basic RIST Testing
```bash
# Test RIST bonding under 4G conditions
bench-cli run --scenario mobile_4g --duration 60 --metrics-output 4g_test.csv

# Compare performance across different scenarios
bench-cli batch-test --scenarios mobile_4g,mobile_5g,fiber_link --iterations 3
```

### Network Degradation Testing
```bash
# Test with increasing packet loss
bench-cli run --config degradation_test.json --duration 180

# Where degradation_test.json defines scheduled loss increases
```

### Performance Benchmarking
```bash
# Run comprehensive benchmark suite
bench-cli test-suite --config benchmark_suite.json --output-dir ./benchmarks

# Generate performance report
bench-cli report --input-dir ./benchmarks --format html
```

## Integration

The CLI integrates with:
- `scenarios`: Predefined network conditions and test cases
- `netns-testbench`: Linux network namespace testing backend
- `netns-testbench`: Network namespace simulation backend
- `observability`: Metrics collection and monitoring
- `rist-elements`: GStreamer RIST element testing

## Output and Reporting

- **CSV Metrics**: Time-series performance data
- **JSON Results**: Structured test results and statistics
- **HTML Reports**: Visual performance reports with charts
- **Prometheus Metrics**: Real-time monitoring integration
- **Console Output**: Live test progress and summary statistics

## Testing Features

The CLI includes comprehensive testing capabilities with:
- Automated assertions on performance metrics
- Regression testing against baseline results
- Load testing with configurable scenarios
- Stress testing under extreme conditions
- Integration testing across all backend systems