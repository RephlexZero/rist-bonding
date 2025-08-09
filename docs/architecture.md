# CI/CD Test Architecture

This document describes the architecture and design decisions for the RIST bonding CI/CD test system.

## Overview

The test system validates the `ristdispatcher` element's load balancing capabilities across multiple variable, lossy network links. It uses Linux network namespaces and traffic control (`tc`) to emulate real-world cellular network conditions.

## Architecture Components

### 1. Network Simulation Layer

**Location**: `netsim/`

- **`setup_network.sh`**: Creates network namespaces and virtual ethernet pairs
- **`tc_control.sh`**: Manages traffic shaping (bandwidth, latency, loss)  
- **`run_scenario.sh`**: Orchestrates complete scenario execution

**Design Principles**:
- Uses Linux network namespaces for isolation
- Veth pairs provide dedicated links between sender/receiver
- TC (traffic control) provides realistic network conditions
- Deterministic and reproducible across CI runs

### 2. Test Scenarios

**Location**: `scenarios/`

Each scenario is defined as a YAML file with:
- Link configuration over time (bandwidth, latency, loss)
- Expected capacity and utilization patterns
- Acceptance criteria thresholds
- Pipeline configuration

**Scenario Types**:
- **S0**: Baseline sanity test (optimal conditions)
- **S1**: Variable bandwidth with staggered transitions
- **S2**: Burst loss patterns on rotating links
- **S3**: Complete link outages and recovery
- **S4**: Asymmetric latency/capacity profiles

### 3. GStreamer Pipelines

**Sender Pipeline**:
```
videotestsrc → x264enc → rtph264pay → ristdispatcher → 4x ristsink
```

**Receiver Pipeline**:
```
4x ristsrc → aggregation → fakesink (with metrics)
```

**Key Design Points**:
- Deterministic test patterns for consistent results
- RIST protocol for forward error correction and adaptive streaming
- Multiple parallel paths through dedicated IP addresses
- Metrics extraction via GStreamer tracers and debug logs

### 4. Metrics Processing

**Location**: `metrics/`

- **`process_scenario.py`**: Parses logs and computes KPIs
- **`generate_final_report.py`**: Combines multi-scenario results

**KPI Categories**:
- **Throughput**: Delivered bitrate vs. available capacity
- **Reliability**: Loss rate after RIST recovery mechanisms  
- **Responsiveness**: Stall duration and recovery times
- **Load Balancing**: Traffic distribution across links

### 5. CI Integration

**Location**: `.github/workflows/ci-netem.yml`

- Builds plugin in release mode
- Installs system dependencies (GStreamer, iproute2)
- Runs scenario suite (subset for PRs, full suite nightly)
- Processes results and uploads artifacts
- Gates PR merges based on acceptance criteria

## Network Topology

```
┌─────────────────┐                    ┌─────────────────┐
│   ns_sender     │                    │  ns_receiver    │
│                 │                    │                 │
│  ┌─────────┐    │    vethS1/vethR1   │    ┌─────────┐  │
│  │ sender  ├────┼────────────────────┼────┤receiver │  │
│  │pipeline │    │    10.0.1.1/30     │    │pipeline │  │
│  │         ├────┼─── 10.0.1.2/30 ────┼────┤         │  │
│  │         │    │                    │    │         │  │
│  │         ├────┼────vethS2/vethR2───┼────┤         │  │
│  │         │    │    10.0.2.1/30     │    │         │  │
│  │         ├────┼─── 10.0.2.2/30 ────┼────┤         │  │
│  │         │    │                    │    │         │  │
│  │         ├────┼────vethS3/vethR3───┼────┤         │  │
│  │         │    │    10.0.3.1/30     │    │         │  │
│  │         ├────┼─── 10.0.3.2/30 ────┼────┤         │  │
│  │         │    │                    │    │         │  │
│  │         ├────┼────vethS4/vethR4───┼────┤         │  │
│  │         │    │    10.0.4.1/30     │    │         │  │
│  └─────────┘    │    10.0.4.2/30     │    └─────────┘  │
└─────────────────┘                    └─────────────────┘
```

Each veth pair represents an independent "cellular" link with:
- Dedicated IP subnet (10.0.X.0/30)
- Individual traffic shaping controls
- Isolated routing for path segregation

## Traffic Control Implementation

### HTB (Hierarchical Token Bucket)
- **Purpose**: Bandwidth limiting per link
- **Configuration**: Root qdisc → class → rate/ceil limits
- **Dynamic**: Updated via `tc class change` during scenarios

### NETEM (Network Emulation)
- **Purpose**: Latency, jitter, and loss simulation  
- **Configuration**: Applied as child qdisc to HTB classes
- **Parameters**: Delay (ms), jitter (ms), loss (%)

### Example TC Configuration:
```bash
# Create HTB root and class
tc qdisc add dev vethS1 root handle 1: htb default 10
tc class add dev vethS1 parent 1: classid 1:10 htb rate 1500kbit ceil 1500kbit

# Add netem for impairments
tc qdisc add dev vethS1 parent 1:10 handle 10: netem delay 20ms 5ms loss 0.5%
```

## Acceptance Criteria

Tests must meet these thresholds to pass:

| Metric | Threshold | Purpose |
|--------|-----------|---------|
| Delivered Bitrate | ≥85% of capacity | Efficiency validation |
| Loss After Recovery | ≤1.0% | RIST effectiveness |
| Max Stall Duration | ≤500ms | Responsiveness |
| Load Balance | Top-2 links ≥70% | Distribution validation |

### Rationale:
- **85% bitrate**: Accounts for protocol overhead and realistic efficiency
- **1% loss**: Validates RIST FEC and retransmission effectiveness
- **500ms stalls**: Ensures responsive adaptation to link changes
- **70% top-2**: Verifies proportional utilization of best links

## Scalability and Performance

### CI Resource Usage:
- **Runtime**: ~15 minutes for PR suite, ~30 minutes nightly
- **Memory**: ~2GB for concurrent GStreamer pipelines  
- **Network**: Contained within netns, no external dependencies
- **Storage**: ~100MB artifacts per run

### Optimization Strategies:
- Cargo build caching reduces compilation time
- Parallel scenario execution where feasible
- Artifact retention limited to 7 days
- Log truncation to prevent excessive output

## Reliability Measures

### Deterministic Testing:
- Fixed test patterns (videotestsrc with ball pattern)
- Precise timing via `tc` schedules with timestamps
- Seeded random components for reproducibility
- Isolated network environments

### Error Handling:
- Comprehensive cleanup on failure or interrupt
- Graceful degradation when components unavailable
- Detailed logging for debugging
- Artifact preservation for post-mortem analysis

### Flake Mitigation:
- Sliding window KPIs smooth over temporary fluctuations
- Multiple measurement samples reduce noise sensitivity  
- Reasonable acceptance thresholds account for variance
- Background process isolation prevents interference

## Future Extensibility

### Planned Enhancements:
- **Matrix Testing**: Multiple GStreamer versions, encoder types
- **Baseline Comparisons**: Single-link vs. bonding performance deltas
- **Chaos Engineering**: Seeded randomness with reproducible failures
- **Performance Trending**: Historical KPI tracking and regression detection

### Architecture Support:
- Modular scenario definitions enable easy new test addition
- Pluggable metrics processors support new KPI types
- Standardized interfaces between network/pipeline/metrics layers
- CI workflow matrix ready for multi-dimensional testing

## Debugging and Observability

### Debug Outputs:
- GStreamer debug logs with configurable verbosity
- Network statistics snapshots at scheduled intervals
- Detailed error messages with context
- Machine-readable metrics for automated analysis

### Troubleshooting Tools:
- Manual network setup scripts for interactive debugging
- Component isolation (network-only, pipeline-only testing)
- Verbose logging modes for deep investigation
- Step-by-step execution guides

This architecture provides a robust, scalable foundation for validating RIST bonding performance under realistic network conditions while maintaining CI/CD integration efficiency.
