# RIST Bonding Plugins

Notes for the patched GStreamer RIST elements and the Rust plugin shipped in this workspace.

## Purpose

- Extend upstream `ristsrc`/`ristsink` with session-level telemetry required by the Rust scheduler.
- Provide bonding-aware Rust elements (`ristdispatcher`, `dynbitrate`, helpers) that consume that telemetry.
- Offer test-only elements to validate pipelines without external dependencies.

## Patched GStreamer Elements (C)

The overlay under `gstreamer/subprojects/gst-plugins-bad/gst/rist/` is kept in lockstep with upstream and layered with the following additions:

- **Receiver reports surfaced**: `rist/x-sender-session-stats` now publishes `rr-fraction-lost`, `rr-extended-highest-seq`, `rr-packets-lost`, and a normalized RTT value (fallback to DLSR when RTT is zero) so the Rust dispatcher can estimate delivered capacity (`gstristsink.c:813`).
- **Session aggregation**: `rist/x-sender-stats` totals original and retransmitted packets to expose goodput vs repair ratios per bonding leg.
- **Receiver-side counters**: `rist/x-receiver-stats` includes the peer addresses plus jitterbuffer recovery counts, duplicates, and RTX round-trip time (`gstristsrc.c:777`).
- **Debug hooks**: `RIST_DEBUG_RR=1` dumps raw receiver reports when telemetry is missing and during troubleshooting (`gstristsink.c:867`, `909`).
- **Stats cadence**: Honour `stats-update-interval` by scheduling periodic prints with `gst_clock_new_periodic_id`, allowing live sampling without attaching probes.

Build the overlay with `./build_gstreamer.sh` (installs under `/usr/local/lib/gstreamer-1.0`) or mirror the Meson flags from that script for custom prefixes. Re-run `ldconfig` after installation.

## Rust Plugin (`crates/rist-elements`)

The Rust crate produces `libgstristelements.so` containing production and testing elements.

| Element | Purpose | Notes |
| --- | --- | --- |
| `ristdispatcher` | Schedules RTP across bonded sessions | SWRR by default, optional DRR (`scheduler=drr`), epsilon probing (`probe-ratio`, `probe-boost`, `probe-period-ms`), metrics bus export |
| `dynbitrate` | Coordinates bitrate with observed congestion | PID-like controller, optional dispatcher coupling via `dispatcher` property |
| `counter_sink`, `test_source`, `stats_monitor` | Test-only helpers | Compiled when `--features test-plugin` is enabled |

### `ristdispatcher` Highlights

- Reads telemetry from the patched `ristsink` via `rist/x-sender-session-stats`.
- Supports initial weight seeding through `weights`, then adapts using SWRR or DRR.
- Micro-probing keeps learning under low traffic; configure with `probe-boost` and `probe-period-ms`.
- Emits metrics on the bus when `metrics-export-interval-ms > 0`.

### `dynbitrate` Highlights

- Targets packet loss and RTT (`target-loss`, `target-rtt`) with bounded step changes (`step-size`).
- Updates upstream encoder bitrate directly (`bitrate` property) and can push hints to `ristdispatcher` when linked.
- Uses smoothing windows to avoid oscillations during transient loss.

### Build & Registration

```bash
# Release build with everything enabled
cargo build -p rist-elements --release --all-features

# Local plugin discovery
export GST_PLUGIN_PATH=$PWD/target/release:$GST_PLUGIN_PATH
gst-inspect-1.0 ristdispatcher

# Optional system-wide install
sudo cp target/release/libgstristelements.so /usr/lib/gstreamer-1.0/
```

For testing elements add `--features test-plugin` before the `cargo build` command.

## Debugging Cheatsheet

```bash
# Verbose logging for Rust elements and patched C plugin
export GST_DEBUG=ristdispatcher:5,dynbitrate:4,rist*:5
export RIST_DEBUG_RR=1

# Observe dispatcher metrics on the bus
gst-launch-1.0 ... ristdispatcher metrics-export-interval-ms=1000 ...

# Inspect live stats structure
GST_DEBUG=ristdispatcher:5 gst-launch-1.0 ... |& grep "ristdispatcher stats"
```

Cross-reference the workspace README for the full build flow and deployment checklist.
