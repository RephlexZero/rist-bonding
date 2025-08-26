# demos# demos



Demo programs showcasing network simulation and RIST testing capabilities.Demo programs showcasing network simulation backends and RIST testing capabilities.



## Overview## Overview



This crate contains demonstration programs that showcase how to use the RIST bonding testbench ecosystem. These demos serve as examples for users learning the system and provide working demonstrations of the `scenarios` and `netns-testbench` integration.This crate contains demonstration programs that showcase the capabilities of the RIST bonding testbench ecosystem. These demos serve as both examples for users learning the system and validation tools for developers working on the various backend implementations.



## Available Demos## Features



### `test_netns_demo`- **Backend Demonstrations**: Examples using the netns-testbench backend

Demonstrates the netns-testbench backend with network namespace isolation.- **Scenario Showcases**: Demonstrations of various network conditions and presets

- **RIST Protocol Examples**: Real-world RIST bonding and failover scenarios

```bash- **Network Scenario Simulation**: Pre-configured scenarios for common RIST use cases

# Requires sudo for network namespace operations- **Real-time Performance Analysis**: Live monitoring of network conditions and adaptation

sudo cargo run --bin test_netns_demo- **Performance Comparisons**: Side-by-side backend performance comparisons

```

## Available Demos

**Features:**

- Network namespace isolation### Network Simulation Demos

- Realistic traffic control using Linux TC

- Multi-link topology setup#### `test_network_sim_demo`

- Network impairment modeling (latency, loss, bandwidth limits)Demonstrates the netns-testbench backend capabilities.

- Integration with scenarios crate



```bash
enhanced_schedulercargo run --bin test_network_sim_demo
```
Demonstrates dynamic network condition scheduling and parameter updates.
# demos

Demo programs for the RIST bonding testbench. These show how to use the `scenarios` and `netns-testbench` crates and include a playable end-to-end media demonstration.

## Binaries

- test_netns_demo
  - Sets up four bonded links with constant bandwidth and loss via Linux network namespaces
  - Sends a 1080p60 video with sine audio over RIST (RTP/MP2T) and writes a 30-second MP4
  - Output file: `target/test-artifacts/demo_bonded_output.mp4`

- enhanced_scheduler
  - Demonstrates dynamic network scheduling with the netns testbench

## Requirements

- Linux with network namespaces and `tc` (iproute2)
- Root or CAP_NET_ADMIN to create namespaces/qdiscs (use `sudo -E` to preserve env)
- GStreamer with the RIST plugin (ristsrc/ristsink)
  - Either installed system-wide, or built via the `gstreamer/` subproject in this repo
  - Ensure the plugin is discoverable (e.g. set `GST_PLUGIN_PATH` accordingly)

## Running the media demo

This starts 4 links with fixed bandwidth/loss in netns, bonds them with RIST, and writes a 30-second MP4.

Steps:
1) Build the workspace (this also builds the demo binary):
   - cargo build -p demos --features netns-sim
2) Make sure the GStreamer RIST plugin is available at runtime:
   - If using system packages, nothing to do
   - If building from this repo, point `GST_PLUGIN_PATH` to the built plugin directory
3) Run the demo with network privileges:
   - sudo -E cargo run -p demos --features netns-sim --bin test_netns_demo

Result: `target/test-artifacts/demo_bonded_output.mp4` should be created. The demo runs ~30 seconds and finalizes the MP4 cleanly (EOS).

Notes:
- The demo encodes H.265 (x265enc) and AAC, muxes to MPEG-TS, payloads as RTP (MP2T), and transports via RIST bonding.
- All lossiness and bandwidth limits are configured by the network namespaces; the app does not vary conditions itself.
- The receiver performs depayload, demux, parsing, and MP4 muxing before writing to disk.

## Live stats and logging

- The demo prints concise RIST stats once per second for the sender and receiver, derived from the `stats` property on `ristsink`/`ristsrc`. Example line format:
  - `[ristsink0] rist/x-sender-stats: sent=..., rtx=...`
  - `[ristsrc0] rist/x-receiver-stats: rx=..., drop=..., dup=..., rtx_req=..., rtt=...ms`

## Known Issues

### Sticky Event Misordering Warnings

You may see GStreamer warnings like:
```
GStreamer-WARNING: Sticky event misordering, got 'segment' before 'caps'
```

These warnings originate from the internal pipeline structure of the RIST plugin elements (`rist_rtx_funnel`, `rist_rtp_de_ext`, etc.) and are benign - they do not affect the functionality of the demo or the quality of the output. The RIST plugin's internal elements receive segment events before caps events due to the complex multi-session bonded pipeline structure.

**Root Cause**: The RIST plugin creates an internal pipeline with elements like:
- `rtpbin` (main RTP handling)  
- `rtxbin` containing `rtx_funnel` and `ristrtpdeext`
- Multiple RTP sessions for bonding

The event ordering issue occurs when these internal elements are connected and GStreamer's event flow doesn't guarantee that caps events reach all elements before segment events in this complex topology.

**Impact**: None - the demo produces correct output and the MP4 file is valid.

**Solution**: These warnings are cosmetic and cannot be easily suppressed without significant changes to the RIST plugin's C code. They can be safely ignored as they do not affect functionality.

**For Cleaner Logs**: If the warnings are distracting during development, you can redirect stderr:
```bash
sudo -E cargo run -p demos --features netns-sim --bin test_netns_demo 2>/dev/null
```
Or filter them out while keeping other important messages:
```bash
sudo -E cargo run -p demos --features netns-sim --bin test_netns_demo 2>&1 | grep -v "misordering"
```