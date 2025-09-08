# RIST dispatcher notes

## Updates

- ristsink now includes receiver report fields per session in `rist/x-sender-session-stats`:
  - `rr-packets-received` (monotonic)
  - `rr-fraction-lost` (if available)

- The Rust dispatcher prefers receiver-delivered rate for capacity estimates:
  - capacity ≈ delivered_pps / last_share (falls back to sender goodput when RR missing)

- Added micro-probe controls to keep learning under light load:
  - `probe-boost` (default 0.12)
  - `probe-period-ms` (default 800)

- Existing `probe-ratio` epsilon mix and `max-link-share` cap are preserved.

# GStreamer Elements for RIST Bonding

This workspace provides advanced GStreamer elements focused on RIST (Reliable Internet Stream Transport) bonding and adaptive control. The elements are implemented in Rust and exposed through the `rist-elements` crate.

## Overview

The RIST bonding implementation extends standard RIST functionality with multi-path capabilities, enabling resilient video streaming over multiple network connections with automatic load balancing and failover.

## Provided Elements

### Core Elements

- **`ristdispatcher`** – Intelligently distributes RTP streams across multiple RIST sessions using advanced load balancing algorithms
- **`dynbitrate`** – Monitors network statistics and adaptively adjusts encoder bitrate while coordinating with dispatcher
- **`ristsrc`** – Enhanced RIST stream receiver with bonding support and statistics collection
- **`ristsink`** – RIST stream sender with per-session statistics and bonding coordination

### Testing Elements (test-plugin feature)

- **`counter_sink`** – Buffer counting sink for testing and validation
- **`test_source`** – Configurable test source with controllable patterns
- **`stats_monitor`** – Statistics collection and reporting for test scenarios

## Element Descriptions

### `ristdispatcher`

The core bonding element that sits between an encoder and multiple `ristsink` elements, implementing sophisticated multi-path distribution.

**Algorithm**: Smooth Weighted Round Robin (SWRR) with automatic rebalancing
**Input**: Single RTP stream from encoder/payloader
**Output**: Multiple RTP streams distributed to RIST sessions

#### Key Features

- **Dynamic Load Balancing**: Automatically adjusts traffic distribution based on real-time RIST statistics
- **Intelligent Failover**: Detects link failures through statistics monitoring and redistributes traffic
- **Keyframe Optimization**: Optional keyframe duplication across links for faster stream recovery  
- **Statistics Integration**: Polls RIST sink elements for RTT, packet loss, and retransmission metrics
- **Hysteresis Control**: Prevents weight flapping through configurable stability windows

#### Properties

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `weights` | `Array<f32>` | `[1.0]` | Per-pad traffic distribution weights |
| `auto-balance` | `bool` | `true` | Enable automatic weight adjustment based on statistics |
| `failover-timeout` | `u32` | `5000` | Link failure detection timeout (milliseconds) |
| `rebalance-interval` | `u32` | `1000` | Statistics polling and weight update interval (ms) |
| `keyframe-duplicate` | `bool` | `false` | Duplicate keyframes across all active links |
| `enable-stats` | `bool` | `false` | Enable detailed per-pad statistics collection |
| `hysteresis-window` | `f32` | `0.1` | Minimum weight change threshold to prevent flapping |

#### Statistics Monitoring

The dispatcher integrates with RIST sink statistics to make intelligent routing decisions:

- **RTT (Round Trip Time)**: Lower RTT receives higher weight
- **Packet Loss**: Higher loss reduces link weight significantly  
- **Retransmissions**: Excessive retransmissions indicate congestion
- **Buffer Health**: Monitors sink buffer levels for congestion detection

#### Example Configuration

```bash
# Automatic balancing with custom initial weights
gst-launch-1.0 \
  videotestsrc ! x264enc ! rtph264pay ! \
  ristdispatcher weights="0.6,0.4" auto-balance=true rebalance-interval=500 name=d \
  d.src_0 ! ristsink uri=rist://primary:1968 \
  d.src_1 ! ristsink uri=rist://backup:1968
```

### `dynbitrate`

A control element that implements adaptive bitrate streaming by monitoring RIST network statistics and adjusting upstream encoder settings.

**Algorithm**: PID-style controller with configurable targets and step sizes
**Integration**: Works independently or in coordination with `ristdispatcher`
**Scope**: Controls encoder bitrate property and optionally dispatcher weights

#### Key Features

- **Statistics-Driven Control**: Uses real-time packet loss, RTT, and retransmission data
- **Encoder Integration**: Drives the `bitrate` property of upstream H.264/H.265 encoders
- **Unified Control**: Optionally coordinates with `ristdispatcher` for joint bitrate/routing optimization
- **Gentle Adjustment**: Configurable step sizes and rate limiting prevent abrupt changes
- **Target-Based**: Operates against configurable packet loss and RTT targets

#### Properties

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `target-loss` | `f32` | `1.0` | Target packet loss percentage (0.0-100.0) |
| `target-rtt` | `u32` | `100` | Target round-trip time in milliseconds |
| `step-size` | `f32` | `10.0` | Bitrate adjustment step size as percentage |
| `min-bitrate` | `u32` | `100000` | Minimum allowed bitrate (bits per second) |
| `max-bitrate` | `u32` | `10000000` | Maximum allowed bitrate (bits per second) |
| `adjustment-interval` | `u32` | `2000` | Statistics polling and adjustment interval (ms) |
| `dispatcher` | `Object` | `null` | Reference to ristdispatcher for coordinated control |

#### Control Algorithm

The `dynbitrate` element implements a sophisticated control loop:

1. **Statistics Collection**: Polls RIST sink(s) for current network metrics
2. **Target Comparison**: Compares actual vs target loss/RTT values  
3. **Decision Making**: Determines whether to increase, decrease, or maintain bitrate
4. **Step Calculation**: Applies configurable step size with rate limiting
5. **Encoder Update**: Sets new bitrate on upstream encoder element
6. **Dispatcher Coordination**: Optionally updates dispatcher weights based on same metrics

#### Usage Patterns

```bash
# Independent bitrate control
gst-launch-1.0 \
  videotestsrc ! \
  x264enc name=encoder bitrate=2000 ! \
  dynbitrate target-loss=0.5 target-rtt=50 ! \
  rtph264pay ! \
  ristsink uri=rist://example.com:1968

# Coordinated with dispatcher
gst-launch-1.0 \
  videotestsrc ! \
  x264enc name=encoder bitrate=3000 ! \
  dynbitrate target-loss=1.0 dispatcher=d ! \
  rtph264pay ! \
  ristdispatcher name=d auto-balance=false \
  d.src_0 ! ristsink uri=rist://primary:1968 \
  d.src_1 ! ristsink uri=rist://backup:1968
```

## Advanced Usage

### Multi-Element Coordination

When `dynbitrate` and `ristdispatcher` are used together, they implement unified control:

1. **Shared Statistics**: Both elements poll the same RIST sink statistics
2. **Coordinated Response**: Bitrate and routing decisions are made jointly
3. **Conflict Prevention**: Dispatcher auto-balance is typically disabled when using dynbitrate coordination
4. **Unified Targets**: Both elements work toward the same network quality targets

### Testing Elements

#### `counter_sink`

A specialized testing sink that counts buffers, events, and provides controllable behavior for test scenarios.

**Properties:**
- `count-eos`: Count EOS events received
- `count-flush`: Count flush events received  
- `drop-probability`: Probability of dropping buffers (for testing)
- `delay-ms`: Artificial processing delay

```bash
# Test pipeline with counter sink
gst-launch-1.0 \
  videotestsrc num-buffers=100 ! \
  ristdispatcher name=d \
  d.src_0 ! counter_sink count-eos=true name=counter
```

## Performance Characteristics

### Throughput
- **ristdispatcher**: >100 Mbps aggregate tested
- **dynbitrate**: Minimal processing overhead (~1% CPU)
- **Memory usage**: Zero-copy forwarding where possible

### Latency  
- **ristdispatcher**: <1ms additional latency for packet distribution
- **dynbitrate**: No buffer processing, control-only
- **Statistics polling**: Configurable intervals (500ms-5000ms recommended)

### Scalability
- **ristdispatcher**: Supports 2-8 output paths efficiently  
- **dynbitrate**: Single encoder control per element
- **Resource usage**: Linear scaling with number of paths

## Building and Installation

### Prerequisites
```bash
# Ubuntu/Debian
sudo apt-get install libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev

# Fedora/RHEL  
sudo dnf install gstreamer1-devel gstreamer1-plugins-base-devel
```

### Build and Install
```bash
# Build plugin
cargo build --release --all-features

# System installation
sudo cp target/release/libgstristelements.so /usr/lib/gstreamer-1.0/

# Local testing
export GST_PLUGIN_PATH=$PWD/target/release
gst-inspect-1.0 ristdispatcher
```

## Debugging and Troubleshooting

### Debug Logging
```bash
# Enable element-specific logging
export GST_DEBUG=ristdispatcher:5,dynbitrate:4,rist*:3

# Statistics debugging
export GST_DEBUG=ristdispatcher:5,rist*:5
```

### Common Issues

**No statistics updates**: Ensure RIST sink elements support statistics and are properly configured
**Weight flapping**: Increase hysteresis-window or rebalance-interval
**High CPU usage**: Reduce statistics polling intervals or disable detailed stats collection
**Buffer drops**: Check network conditions and consider adjusting weights or bitrate targets

For detailed troubleshooting, see the integration tests in the `rist-elements` crate.
