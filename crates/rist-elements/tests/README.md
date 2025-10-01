# rist-elements Test Suite

Structure and usage notes for the bonding test harness.

## Layout

```
tests/
├── unit/                        # Pure Rust unit tests (scheduler math, helpers)
├── stress/                      # Long-running validation (pad churn, stats polling)
├── scenarios/                   # Multi-stage bonding stories
├── integration/                 # GStreamer pipelines with namespaces
├── bonded_links_static_stress.rs# Convergence check under fixed capacities
├── static_bandwidths_networks_test.rs
├── quad_links_bonding_modes.rs
├── realistic_network_evaluation.rs
└── ... (entry points: unit_tests.rs, stress_tests.rs, scenario_tests.rs, integration_tests.rs)
```

The standalone `.rs` files at the top level are invoked directly by cargo (e.g. `bonded_links_static_stress`). They provide targeted checks outside the generic entry points.

## Running Tests

```bash
# Everything (requires patched GStreamer + CAP_NET_ADMIN)
cargo test -p rist-elements --all-features

# Unit layer only (fast, no namespaces)
cargo test -p rist-elements unit_tests

# Integration pipelines (creates namespaces and veth pairs)
sudo -E cargo test -p rist-elements integration_tests -- --nocapture

# Static convergence showcase used in docs
cargo test -p rist-elements bonded_links_static_stress -- --nocapture
```

The namespace-oriented suites (`integration`, `scenarios`, `stress`) call into the `network-sim` crate to shape traffic. Running them without elevated privileges results in a permission error—rerun with `sudo -E` or inside the devcontainer.

## Test Artifacts

Outputs (metrics, debug JSON) are written to `target/test-artifacts/` unless `TEST_ARTIFACTS_DIR` is defined. Cleanups run automatically, but you can delete the directory before re-running:

```bash
rm -rf target/test-artifacts
```

## Tips

- Export `RUST_LOG=rist_elements=debug,network_sim=info` to trace scheduling and TC actions concurrently.
- Use `GST_DEBUG=ristdispatcher:5,rist*:4` when reproducing pipeline-level issues.
- If a test aborts, prune stray namespaces with `sudo ip netns delete rist-sender rist-receiver 2>/dev/null || true` before rerunning.

See `/docs/testing/README.md` for broader guidance covering CI flows and environment requirements.
