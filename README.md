# gst-rist-smart

A Rust GStreamer plugin crate providing two custom elements for intelligent RIST streaming:

- **ristdispatcher** — A dispatcher element for `ristsink`'s `dispatcher` property that routes RTP packets across multiple RIST sessions using NACK/RTT-aware load balancing (not just round-robin).
- **dynbitrate** — A controller element that monitors `ristsink` stats and dynamically adjusts an upstream encoder's bitrate to maintain stability across bonded cellular links.

## Features

- **Smart Load Balancing**: Routes packets based on link health (RTT, packet loss) rather than simple round-robin
- **Dynamic Bitrate Control**: AIMD-based rate control that adapts encoder bitrate based on RIST session statistics
- **Cellular Link Optimization**: Designed for bonded cellular connections with automatic keyframe insertion on bitrate reductions

## Building

This project requires GStreamer 1.24+ and the GStreamer Rust bindings.

```bash
# Install GStreamer development packages (Ubuntu/Debian)
sudo apt install libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev

# Build the plugin
PKG_CONFIG_PATH=/usr/lib/pkgconfig cargo build --release

# The plugin will be built as target/release/libgstristsmart.so
```

## Installation

Copy the built plugin to your GStreamer plugin directory:

```bash
# System-wide installation
sudo cp target/release/libgstristsmart.so /usr/lib/gstreamer-1.0/

# Or set GST_PLUGIN_PATH for local usage
export GST_PLUGIN_PATH=$PWD/target/release:$GST_PLUGIN_PATH
```

## Troubleshooting

If the plugin doesn't load, check:

```bash
# Verify the plugin loads
GST_DEBUG=2 GST_PLUGIN_PATH=target/debug gst-inspect-1.0 2>&1 | grep -i gstristsmart

# Test individual elements (when plugin loads correctly)
GST_PLUGIN_PATH=target/debug gst-inspect-1.0 ristdispatcher
GST_PLUGIN_PATH=target/debug gst-inspect-1.0 dynbitrate
```

Note: The plugin registration may need additional fixes for proper GStreamer integration. The core elements are implemented but may require debugging for full functionality.

## Usage

### Basic RIST Sender with Smart Dispatcher

```bash
GST_PLUGIN_PATH=$PWD/target/debug \
gst-launch-1.0 \
  v4l2src device=/dev/video0 ! videoconvert ! \
  x265enc tune=zerolatency key-int-max=60 bitrate=4000 ! h265parse config-interval=-1 ! \
  rtph265pay pt=96 mtu=1200 aggregate-mode=zero-latency ! \
  ristsink \
    dispatcher=ristdispatcher \
    bonding-addresses="10.0.0.2:5004,10.0.1.2:5004,10.0.2.2:5004" \
    stats-update-interval=500
```

### RIST Receiver

```bash
gst-launch-1.0 \
  ristsrc address=0.0.0.0 port=5004 encoding-name="H265" \
    bonding-addresses="0.0.0.0:5004,0.0.0.0:5006,0.0.0.0:5008" ! \
  rtph265depay ! h265parse ! avdec_h265 ! autovideosink sync=false
```

## Element Properties

### ristdispatcher

- `weights`: JSON array of initial link weights, e.g., `"[1.0, 0.8, 1.2]"`
- `rebalance-interval-ms`: How often to recompute weights from stats (default: 500ms)
- `strategy`: Weight update strategy - "aimd" or "ewma" (default: "ewma")

### dynbitrate

- `encoder`: Reference to the encoder element whose bitrate to control
- `rist`: Reference to the ristsink element to read stats from
- `min-kbps`: Minimum bitrate in kbps (default: 500)
- `max-kbps`: Maximum bitrate in kbps (default: 8000)
- `step-kbps`: Bitrate adjustment step size (default: 250)
- `target-loss-pct`: Target packet loss percentage (default: 0.5%)
- `min-rtx-rtt-ms`: Minimum RTT threshold in milliseconds (default: 40)
- `downscale-keyunit`: Force keyframe on bitrate reduction (default: true)

## License

This project is licensed under either of

- Apache License, Version 2.0
- MIT License

at your option.
