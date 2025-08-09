# Local Testing Guide

This guide explains how to run the RIST bonding tests locally for development and debugging.

## Prerequisites

### System Requirements

- Linux system with root access (for network namespaces)
- Ubuntu 20.04+ recommended (other distros may work with package name adjustments)

### Required Packages

```bash
# GStreamer development and runtime
sudo apt-get install -y \
  libgstreamer1.0-dev \
  libgstreamer-plugins-base1.0-dev \
  libgstreamer-plugins-bad1.0-dev \
  gstreamer1.0-plugins-base \
  gstreamer1.0-plugins-good \
  gstreamer1.0-plugins-bad \
  gstreamer1.0-plugins-ugly \
  gstreamer1.0-libav

# Network utilities
sudo apt-get install -y \
  iproute2 \
  netcat-openbsd \
  jq

# Python for metrics processing
sudo apt-get install -y \
  python3 \
  python3-pip \
  python3-yaml

# Build tools (if not already installed)
sudo apt-get install -y \
  build-essential \
  pkg-config
```

### Rust Toolchain

Install Rust if not already available:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

## Building the Plugin

1. Build the plugin in release mode:

```bash
PKG_CONFIG_PATH=/usr/lib/pkgconfig cargo build --release
```

2. Verify the plugin is built correctly:

```bash
export GST_PLUGIN_PATH="$(pwd)/target/release"
gst-inspect-1.0 ristdispatcher | head -20
gst-inspect-1.0 dynbitrate | head -20
```

You should see plugin information without errors.

## Running Tests

### Quick Test (Single Scenario)

Run a single scenario to verify everything works:

```bash
# Run the baseline scenario (S0)
sudo ./netsim/run_scenario.sh S0 1
```

This will:
1. Set up network namespaces and virtual interfaces
2. Run GStreamer pipelines for 30 seconds
3. Process metrics and generate results
4. Clean up network configuration

### Full Test Suite

Run all scenarios (takes longer):

```bash
# Run all scenarios with standard duration
for scenario in S0 S1 S2 S3 S4; do
    echo "Running scenario $scenario..."
    sudo ./netsim/run_scenario.sh "$scenario" 1
done

# Generate final report
python3 metrics/generate_final_report.py results/
```

### Extended Testing

For more thorough testing with longer durations:

```bash
# Run with 2x duration multiplier
sudo ./netsim/run_scenario.sh S1 2
```

## Understanding the Output

### Directory Structure

After running tests, you'll see:

```
results/
├── S0/
│   ├── metrics.json        # Machine-readable results
│   ├── summary.md          # Human-readable summary  
│   ├── initial_stats.txt   # Network stats at start
│   ├── final_stats.txt     # Network stats at end
│   └── *.txt               # Periodic network snapshots
├── S1/
│   └── ...
└── final_summary.md        # Overall test results
logs/
├── S0/
│   ├── sender.log         # GStreamer sender pipeline log
│   └── receiver.log       # GStreamer receiver pipeline log
└── ...
```

### Reading Results

1. **Quick Status**: Check `results/final_summary.md`
2. **Scenario Details**: Check `results/S*/summary.md` for individual scenario results
3. **Raw Data**: Examine `results/S*/metrics.json` for detailed measurements
4. **Debugging**: Look at `logs/S*/sender.log` and `logs/S*/receiver.log`

### Success Criteria

Tests pass when:
- **Delivered Bitrate**: ≥85% of available capacity
- **Loss Rate**: ≤1% after RIST recovery
- **Max Stall**: ≤500ms
- **Load Balancing**: Top 2 links carry ≥70% of traffic

## Manual Network Testing

For debugging network issues, you can set up the simulation manually:

```bash
# Set up network (as root)
sudo ./netsim/setup_network.sh setup

# Verify connectivity
sudo ip netns exec ns_sender ping -c 3 10.0.1.2
sudo ip netns exec ns_sender ping -c 3 10.0.2.2
sudo ip netns exec ns_sender ping -c 3 10.0.3.2  
sudo ip netns exec ns_sender ping -c 3 10.0.4.2

# Test traffic shaping
sudo ./netsim/tc_control.sh show

# Manually adjust link parameters
sudo ./netsim/tc_control.sh bandwidth 1 500  # Set link 1 to 500 kbps
sudo ./netsim/tc_control.sh latency 1 100    # Set link 1 to 100ms latency
sudo ./netsim/tc_control.sh loss 1 5         # Set link 1 to 5% loss

# Capture statistics
sudo ./netsim/tc_control.sh stats /tmp/manual_test_stats.txt

# Clean up when done
sudo ./netsim/setup_network.sh cleanup
```

## Manual GStreamer Testing

Test GStreamer pipelines manually:

```bash
export GST_PLUGIN_PATH="$(pwd)/target/release"

# In one terminal (receiver)
sudo ip netns exec ns_receiver \
  gst-launch-1.0 -v \
  ristsrc address=10.0.1.2 port=5001 ! fakesink dump=false

# In another terminal (sender)  
sudo ip netns exec ns_sender \
  gst-launch-1.0 -v \
  videotestsrc pattern=ball is-live=true ! \
  video/x-raw,width=320,height=240,framerate=30/1 ! \
  x264enc bitrate=1000 key-int-max=30 ! \
  video/x-h264,profile=baseline ! \
  rtph264pay ! \
  ristsink address=10.0.1.2 port=5001
```

## Troubleshooting

### Common Issues

1. **Permission Denied**
   - Ensure you're running network commands with `sudo`
   - Check that your user can create network namespaces

2. **Plugin Not Found**
   - Verify `GST_PLUGIN_PATH` is set correctly
   - Rebuild with `PKG_CONFIG_PATH=/usr/lib/pkgconfig cargo build --release`
   - Check that `libgstristsmart.so` exists in `target/release/`

3. **Network Connectivity Issues**
   - Run `sudo ./netsim/setup_network.sh verify` to test connectivity
   - Check kernel modules: `sudo modprobe dummy`
   - Verify IP forwarding: `sudo sysctl -w net.ipv4.ip_forward=1`

4. **GStreamer Pipeline Errors**
   - Check logs in `logs/*/sender.log` and `logs/*/receiver.log`
   - Test with simpler pipelines first
   - Verify RIST plugins are available: `gst-inspect-1.0 | grep rist`

5. **Python Dependencies**
   - Install missing packages: `pip3 install pyyaml`
   - Check Python 3 is available: `python3 --version`

### Debug Mode

For more verbose output:

```bash
# Enable GStreamer debug
export GST_DEBUG="rist*:5,dispatcher*:4"

# Run scenario with debug output
sudo -E ./netsim/run_scenario.sh S0 1
```

### Cleaning Up Failed Tests

If a test fails and leaves network configuration:

```bash
# Force cleanup
sudo ./netsim/setup_network.sh cleanup

# Remove any orphaned network interfaces
sudo ip link delete vethS1 2>/dev/null || true
sudo ip link delete vethS2 2>/dev/null || true
sudo ip link delete vethS3 2>/dev/null || true
sudo ip link delete vethS4 2>/dev/null || true

# Remove namespaces if they exist
sudo ip netns delete ns_sender 2>/dev/null || true
sudo ip netns delete ns_receiver 2>/dev/null || true
```

## Performance Considerations

- **VM/Container Limitations**: Network emulation may be less accurate in virtualized environments
- **System Load**: Other processes can affect timing-sensitive tests
- **Disk Space**: Logs can be large with high debug levels
- **Memory**: Multiple GStreamer pipelines require sufficient RAM

## Contributing Test Improvements

When modifying tests:

1. Update scenario YAML files in `scenarios/` for configuration changes
2. Modify `netsim/run_scenario.sh` for execution logic changes
3. Update `metrics/process_scenario.py` for new KPI calculations
4. Test locally before submitting PRs
5. Update this documentation for new requirements or procedures
