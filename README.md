# RIST Bonding Workspace

A unified workspace for bonding-aware RIST streaming: patched GStreamer C plugins, Rust GStreamer elements, and reproducible network simulation tooling designed for low-latency, failure-resilient delivery.

## Overview

- Maintains an overlay of the upstream GStreamer RIST plugin with additional telemetry and bonding hooks used by our Rust stack.
- Ships Rust-based GStreamer elements (`rist-elements`) that schedule traffic across multiple RIST sessions and coordinate bitrate control.
- Provides a traffic-control orchestration library (`network-sim`) to emulate asymmetric and lossy links directly from tests.
- Includes automated stress tests, most notably `bonded_links_static_stress`, to demonstrate convergence toward true link capacities under static load.

## Objectives

- Keep the RIST bonding stack production-ready while exposing the metrics our controllers need (RTCP RTT, retransmissions, loss).
- Offer a fast feedback loop for experimenting with scheduling logic by pairing controllable simulation with instrumented sinks/sources.
- Make handover straightforward: every major component lives in this repository with documented build, install, and validation flows.

## Component Map

| Area | Path | Purpose |
| --- | --- | --- |
| Patched RIST C plugin | `gstreamer/subprojects/gst-plugins-bad/gst/rist/` | Adds telemetry and bonding support to `ristsrc`, `ristsink`, RTX helpers |
| Rust GStreamer plugin | `crates/rist-elements/` | Hosts `ristdispatcher`, `dynbitrate`, optional test elements, and scheduling logic |
| Network simulation | `crates/network-sim/` | Applies Linux TC profiles from tests, provides namespace helpers |
| Helper scripts | `build_gstreamer.sh`, `run_convergence_test.sh` | Automate rebuilds and convergence validation |
| Docs | `docs/plugins/`, `docs/testing/` | Background on dispatcher behavior and test workflows |

## Patched GStreamer RIST Plugin (C)

The project tracks upstream `gst-plugins-bad` but ships a maintained overlay for `ristsrc`/`ristsink` to expose per-link intelligence needed by our Rust controllers.

**Key Enhancements**

- `rist/x-sender-session-stats` now includes RTCP receiver report fields (`rr-fraction-lost`, `rr-extended-highest-seq`, `rr-packets-lost`, `rr-round-trip-time`) plus `rr-have-report` to flag when telemetry is authoritative. (`gstreamer/.../gstristsink.c:813`)
- `rist/x-sender-stats` aggregates original vs retransmitted packet counts per session and overall, giving dispatchers accurate goodput vs. repair rates.
- Computed RTT falls back to DLSR (delay since last sender report) when round-trip time is not yet populated, preventing early-zero readings during warmup. (`gstristsink.c:879`)
- Added `RIST_DEBUG_RR` environment hook to dump raw receiver reports whenever telemetry is missing or when in-depth debugging is requested. (`gstristsink.c:867`, `909`)
- `rist/x-receiver-stats` bundles per-session source addresses together with jitterbuffer totals (`recovered`, `retransmission-requests-sent`, `duplicates`, `rtx-roundtrip-time`) so downstream analytics can differentiate permanent loss from recovered packets. (`gstristsrc.c:777`)
- Both elements honour `stats-update-interval` by scheduling periodic dumps through the system clock, allowing operators to pull structured stats at runtime.

**Build & Install**

The GStreamer submodule is now built automatically by cargo's build system:

1. Ensure submodules are present (the repo already vendors GStreamer):
   ```bash
   git submodule update --init --recursive
   ```
2. Install build dependencies (`meson`, `ninja`, GStreamer dev packages, compiler toolchain).
3. Simply run cargo build - the build.rs script will automatically build GStreamer:
   ```bash
   cargo build --release
   ```
   The build script checks if GStreamer needs building and runs `build_gstreamer.sh` automatically.
   Artifacts land under `target/gstreamer/install` and environment variables are configured automatically.

4. **Manual build option**: You can still run `./build_gstreamer.sh` directly if needed:
   ```bash
   ./build_gstreamer.sh        # pass --clean to force a fresh Meson build
   source target/gstreamer/env.sh  # set environment variables manually
   ```

5. **Force rebuild**: Set environment variable to force GStreamer rebuild:
   ```bash
   FORCE_GSTREAMER_BUILD=1 cargo build --release
   ```

## Rust GStreamer Plugin (`rist-elements`)

`libgstristelements.so` provides the Rust side of the bonding stack:

- **`ristdispatcher`**: Smooth Weighted Round Robin (SWRR) and optional Deficit Round Robin (DRR) packet scheduler with epsilon probing (`probe-ratio`, `probe-boost`, `probe-period-ms`) for continuous learning.
- **`dynbitrate`**: PID-inspired controller that tunes encoder bitrate and can coordinate with `ristdispatcher` to avoid conflicting reactions.
- **Test-only helpers** (`feature = "test-plugin"`): `counter_sink`, `test_source`, `stats_monitor` for deterministic integration tests.

**Build & Register**

```bash
# Release build with all features (needed for production pipelines)
cargo build -p rist-elements --release --all-features

# Local inspection
export GST_PLUGIN_PATH=$PWD/target/release
GST_DEBUG=ristdispatcher:4 gst-inspect-1.0 ristdispatcher

# System-wide install
sudo cp target/release/libgstristelements.so /usr/lib/gstreamer-1.0/
```

The crate’s README (`crates/rist-elements/README.md`) documents every property. Most production pipelines use the default SWRR scheduler with micro-probes enabled.

## Network Simulation (`network-sim`)

- Wraps Linux Traffic Control primitives (HTB + netem) behind async Rust APIs.
- Ships presets (`NetworkParams::good/typical/poor`) aligned with recurring test scenarios.
- Integrates with GStreamer tests through namespace-aware helpers used across `rist-elements` integration suites.

Build or include it by running:
```bash
cargo build -p network-sim --release
```
The crate is consumed directly by Rust tests and external tools; no shared library is produced.

## Building the Full Stack

The build process is **completely automated** - just run `cargo build` and everything happens automatically!

### Quick Start

```bash
# Clone the repo
git clone <repo-url>
cd rist-bonding

# Just build - that's it!
cargo build
```

**What happens automatically:**
1. Git submodules are initialized (if needed)
2. GStreamer is built with patched RIST plugin (if needed)
3. Environment variables are configured (via `.cargo/config.toml`)
4. All Rust crates are built with correct linkage

No manual steps required!

### Makefile Targets (Optional)

For convenience, make targets are also available:

```bash
make build        # Same as cargo build
make gstreamer    # Explicitly rebuild GStreamer
make test         # Run all tests
make verify       # Verify build is working
make clean        # Clean Rust artifacts (keeps GStreamer)
make clean-all    # Clean everything including GStreamer
```

### Environment Setup

The `.cargo/config.toml` automatically configures:
- `PKG_CONFIG_PATH` - Points to built GStreamer for compilation
- `GST_PLUGIN_PATH` - Plugin search paths for runtime
- `LD_LIBRARY_PATH` - Library paths for linking and runtime

For manual environment setup or shell scripts:
```bash
source target/gstreamer/env.sh
```

Or use direnv (`.envrc` is provided):
```bash
direnv allow
```

### Force Rebuild

To force GStreamer to rebuild:
```bash
FORCE_GSTREAMER_BUILD=1 cargo build --release
# Or use the script directly:
./build_gstreamer.sh --clean
```

### Verify Installation

```bash
# Check GStreamer plugins
gst-inspect-1.0 ristsink    # C plugin with telemetry patches
gst-inspect-1.0 ristdispatcher  # Rust plugin

# Enable verbose logging if needed
export GST_DEBUG=rist*:5
```

## Testing & Validation

- Run the bonding suite:
  ```bash
  cargo test -p rist-elements --all-features
  ```
- Validate the GStreamer overlay using the convergence test:
  ```bash
  cargo test -p rist-elements bonded_links_static_stress -- --nocapture
  ```
  This spins up namespaces, applies asymmetric bandwidth ceilings through `network-sim`, and shows the dispatcher converging onto the true link rates. Use `./run_convergence_test.sh` for the repository defaults (GST debug filters and env helper sourcing).
- Exercise network primitives in isolation:
  ```bash
  cargo test -p network-sim --all-features
  ```
- Integration tests that require elevated privileges can be rerun with `sudo -E` (see `run_convergence_test.sh` for the tuned defaults).

## Operational Tips

- Enable verbose telemetry while tuning controllers:
  ```bash
  export GST_DEBUG=ristdispatcher:5,rist*:5
  export RIST_DEBUG_RR=1
  ```
- The dispatcher publishes metrics on the bus when `metrics-export-interval-ms` is set; hook this into your observability pipeline for live dashboards.
- Keep `GST_PLUGIN_PATH` aligned between the patched C plugin and Rust plugin directories to avoid mixing upstream binaries with custom elements.

## Development Environment

- The repo is VS Code devcontainer-ready (`.devcontainer/`), bundling Rust, Meson, GStreamer toolchains, and CAP_NET_ADMIN permissions. Open the folder in VS Code and choose “Reopen in Container” for a fully provisioned environment.

## Handover Checklist

- [ ] Patched GStreamer installed (`gst-inspect-1.0 ristsrc` shows stats fields under `Signals` → `stats`)
- [ ] Rust plugin deployed and visible (`gst-inspect-1.0 ristdispatcher`)
- [ ] `cargo test -p rist-elements bonded_links_static_stress -- --nocapture` reviewed for weight convergence
- [ ] Any production pipelines updated to reference the new plugin paths or `GST_PLUGIN_PATH`

With the steps above, a new environment can rebuild the entire bonding stack, inspect the custom telemetry emitted by the patched C plugin, and rely on repeatable tests to verify bonding performance.
