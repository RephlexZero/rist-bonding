# rist-elements

Rust-based GStreamer elements that implement the bonding logic for the RIST workspace. The crate complements the patched C plugins by consuming their telemetry and coordinating traffic across bonded links.

## Responsibilities

- Provide `ristdispatcher`, a multi-output RTP scheduler aware of per-session stats emitted by the patched `ristsink`.
- Provide `dynbitrate`, a control loop that adjusts upstream encoder bitrate and (optionally) influences dispatcher weights.
- Ship test-only helpers (`counter_sink`, `test_source`, `stats_monitor`) behind the `test-plugin` feature for deterministic integration tests.

## Architecture Notes

- The dispatcher polls `rist/x-sender-session-stats` and `rist/x-receiver-stats` to compute smooth weighted round-robin (SWRR) or deficit round robin (DRR) weights.
- Micro-probing keeps links warm using `probe-ratio`, `probe-boost`, and `probe-period-ms` so the scheduler continues to learn under low load.
- Metrics can be emitted on the bus (`metrics-export-interval-ms`) for external observability systems.
- `dynbitrate` reacts to packet loss and RTT targets and can push hints to the dispatcher through its `dispatcher` property; disable dispatcher auto-balance when using coordinated mode.

## Building the Plugin

```bash
# Build the release plugin with production features
cargo build -p rist-elements --release --all-features

# Expose locally (recommended for development)
export GST_PLUGIN_PATH=$PWD/target/release:$GST_PLUGIN_PATH

gst-inspect-1.0 ristdispatcher

# Optional: install system-wide
sudo cp target/release/libgstristelements.so /usr/lib/gstreamer-1.0/
```

To compile the test plugin elements add `--features test-plugin` to the cargo command.

## Element Cheat Sheet

| Element | Core job | Key properties |
| --- | --- | --- |
| `ristdispatcher` | Bonded RTP scheduler | `weights`, `scheduler` (`swrr`/`drr`), `auto-balance`, `probe-ratio`, `probe-boost`, `probe-period-ms`, `metrics-export-interval-ms` |
| `dynbitrate` | Bitrate controller | `target-loss`, `target-rtt`, `step-size`, `min-bitrate`, `max-bitrate`, `dispatcher` |
| `counter_sink` | Test helper | `count-eos`, `drop-probability`, `delay-ms` |

Common property patterns:

- `weights` accepts either a JSON string (`"[0.6,0.4]"`) or a `Vec<f32>` via the Rust API.
- When `scheduler=drr`, use `quantum-bytes` to size each round; defaults to 1500 bytes.
- `metrics-export-interval-ms > 0` triggers periodic `GstMessage` emissions on the bus with structure name `ristdispatcher-stats`.

## Tests

All tests live under `crates/rist-elements/tests/`. Frequently used targets:

- `cargo test -p rist-elements bonded_links_static_stress -- --nocapture`
  - Canonical convergence showcase built on the network simulator.
- `cargo test -p rist-elements integration_tests -- --nocapture`
  - End-to-end bonding matrix (requires patched GStreamer + CAP_NET_ADMIN).
- `cargo test -p rist-elements stress_tests -- --nocapture`
  - Longer-running drift and failover coverage (same requirements as integration).
- `cargo test -p rist-elements unit_tests`
  - Pure Rust helpers without namespaces.

Run with `sudo -E` if your user lacks `CAP_NET_ADMIN`; the namespace-aware tests exit early with a clear warning otherwise.

## Interaction With Other Crates

- **Patched C plugin**: Ensure the `build_gstreamer.sh` output is installed; otherwise the dispatcher will miss RTCP fields and fall back to degraded heuristics.
- **`network-sim`**: Integration tests depend on the async TC wrappers to shape traffic; run `cargo test -p network-sim --all-features` if scheduler behaviour looks off.
- **Main README**: See `/README.md` for the full bonding stack build order and handover checklist.

## Debugging Tips

```bash
# Enable verbose tracing for both the Rust elements and patched C bits
export GST_DEBUG=ristdispatcher:5,dynbitrate:4,rist*:5
export RIST_DEBUG_RR=1

# Emit metrics every second for external scraping
GST_DEBUG=ristdispatcher:5 gst-launch-1.0 ... ristdispatcher metrics-export-interval-ms=1000 ...
```

The dispatcher also writes structured logs via the Rust `tracing` subscriber. Set `RUST_LOG=rist_elements=debug` to capture them.
