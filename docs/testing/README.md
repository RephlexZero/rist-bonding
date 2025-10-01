# Testing Guide

How to validate the bonding stack end-to-end.

## Test Categories

| Command | Purpose | Notes |
| --- | --- | --- |
| `cargo test --all-features` | Workspace smoke (unit + doc tests) | No namespaces required |
| `cargo test -p rist-elements unit_tests` | Scheduler/unit coverage | Pure Rust |
| `cargo test -p network-sim --all-features` | Traffic-control wrappers | Needs `tc` + CAP_NET_ADMIN |
| `cargo test -p rist-elements integration_tests -- --nocapture` | Full GStreamer pipelines | Creates namespaces, use `sudo -E` or devcontainer |
| `cargo test -p rist-elements bonded_links_static_stress -- --nocapture` | Convergence showcase | Generates per-link metrics proving weight recovery |

The stress and scenario suites (`rist-elements` crate) apply asymmetric network profiles via `network-sim`; the patched GStreamer plugin must be installed so RTT/loss metrics are available.

## Environment Requirements

- Patched `ristsrc`/`ristsink` installed (run `gst-inspect-1.0 ristsink` and confirm `rist/x-sender-session-stats`).
- `GST_PLUGIN_PATH` points to `target/release` (or system install path) so `ristdispatcher` loads.
- Host or devcontainer has `CAP_NET_ADMIN` and `iproute2` for namespace creation.

## Common Environment Variables

```bash
export GST_DEBUG=ristdispatcher:5,rist*:5      # Verbose GStreamer logging
export RUST_LOG=rist_elements=debug            # Rust tracing
export RIST_DEBUG_RR=1                        # Dump RTCP receiver reports from patched C plugin
export TEST_ARTIFACTS_DIR=$PWD/artifacts      # Custom location for logs/metrics
```

Unset `GST_DEBUG` for cleaner CI output once a scenario is stable.

## Useful Scripts

```bash
./run_test.sh                             # Wrapper that builds the stack and runs key suites
./scripts/run_automated_integration_sudo.sh  # Convenience for privileged integration tests
```

Both scripts assume the devcontainer toolchain (Meson, Ninja, Rust) is present.

## Troubleshooting

- **`Operation not permitted`**: rerun with `sudo -E` or ensure the process has `CAP_NET_ADMIN`.
- **`ristsink` missing**: reinstall patched GStreamer via `./build_gstreamer.sh` and restart the shell to pick up `GST_PLUGIN_PATH`.
- **Namespace leftovers**: `sudo ip netns delete rist-sender rist-receiver 2>/dev/null || true`.
- **No convergence in stress test**: verify `RIST_DEBUG_RR=1` shows receiver reports; absence indicates the patched plugin is not in use.

## CI Notes

GitHub Actions runs the unit tests and formatter checks. Namespace-dependent suites are executed in dedicated runners with elevated privileges. Mirror that setup locally using the devcontainer (`code . → Reopen in Container`).

For deeper dives, see:

- `/docs/plugins/README.md` for element specifics.
- `/crates/rist-elements/tests/README.md` for suite layout.
- `/crates/network-sim/README.md` for TC helper behaviour.
