# network-sim

Async helpers for shaping Linux network interfaces with Traffic Control (TC). The crate underpins the bonding stress tests by applying repeatable delay, loss, jitter, and bandwidth limits to veth pairs and namespaces.

## Highlights

- Wraps `tc` interactions in a Tokio-friendly API (`QdiscManager`, `apply_network_params`, `remove_network_params`).
- Provides tuned presets (`NetworkParams::good`, `typical`, `poor`) that mirror the scenarios used in CI.
- Supports namespace-aware operations so tests can prepare isolated topologies without shelling out.
- Ensures cleanup by tracking the qdisc hierarchy it creates (HTB root + netem child).

## Quick Start

```rust
use network_sim::{apply_network_params, remove_network_params, NetworkParams, QdiscManager};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let qdisc = QdiscManager::default();
    let params = NetworkParams { delay_ms: 40, loss_pct: 0.01, rate_kbps: 3_000, jitter_ms: 5, ..Default::default() };

    apply_network_params(&qdisc, "veth-test", &params).await?;
    println!("Simulation active on veth-test");

    // ... run bonding workload ...

    remove_network_params(&qdisc, "veth-test").await?;
    Ok(())
}
```

## Namespace Helpers

Integration tests routinely call `Namespace::create("rist-sender")` to spin up isolated environments. The helper API:

- Creates the namespace and veth pair.
- Applies the requested profile on each side.
- Tears everything down on drop to keep CI machines clean.

See `crates/network-sim/tests/loss_validation.rs` for full examples.

## Building & Testing

```bash
# Build library (no binaries are produced)
cargo build -p network-sim --release

# Run the async test suite (needs CAP_NET_ADMIN)
cargo test -p network-sim --all-features -- --nocapture
```

Many tests expect to run inside the devcontainer or on a host with `iproute2` available. You will see a permission error if the required capabilities are missing; rerun with `sudo -E` in that case.

## How the Bonding Tests Use It

- `bonded_links_static_stress` applies asymmetrical bandwidth ceilings and fixed RTT values to demonstrate dispatcher convergence.
- Unit-level helpers reuse the same `QdiscManager` plumbing for lighter validation in CI.

## Troubleshooting

- Ensure the patched GStreamer stack is installed; otherwise tests may pass but the dispatcher under-measures delivered rate.
- If a test exits unexpectedly, clean up residual namespaces: `sudo ip netns delete rist-sender rist-receiver 2>/dev/null || true`.
- Use `RUST_LOG=network_sim=trace` to observe `tc` commands and timing decisions.

Cross-reference the workspace README for the full bonding build flow and environment checklist.
