# Network-sim crate – Critical Evaluation (2025-09-02)

## Executive summary

The network-sim crate now applies real traffic shaping via Linux tc, replacing the prior no-op. It exposes a simple async API to apply and remove fixed network conditions. However, the current chaining of qdiscs is likely incorrect for rate limiting (uses classless TBF as root with a child qdisc), several README-advertised APIs remain unimplemented, and error handling/reporting is coarse. Tightening the qdisc topology, aligning docs, and improving observability will make it reliable for automated testing.

## What works today
- Actual qdisc application on Linux (requires CAP_NET_ADMIN):
  - Deletes existing root qdisc
  - Applies delay/loss/reorder/duplicate via netem
  - Applies rate limiting via tbf when a rate is set
- Minimal APIs are consistent and async:
  - `apply_network_params(&QdiscManager, iface, &NetworkParams)`
  - `remove_network_params(&QdiscManager, iface)`
- Builds cleanly in the workspace; no compile errors.

## Key issues and gaps
1) Qdisc chaining correctness (rate + netem)
- Current flow: root TBF (classless) → attach netem as child under `parent 1:1`.
- Problem: TBF is classless; you cannot attach a child qdisc to a classless root. The `parent 1:1` class does not exist for TBF, so tc will likely fail when adding netem in this configuration.
- Impact: Rate-limited scenarios will fail to apply; only pure netem-without-rate might work.

2) Dependency/documentation mismatch
- Cargo includes `netlink-packet-route` and `netlink-sys`, but implementation uses `tc` via `tokio::process::Command` and does not use these crates.
- README claims direct netlink usage and richer capabilities than currently implemented.

3) Documentation overpromises
- README references functions/builders not present (examples: `get_interface_stats`, `QdiscConfig`, builder-like `NetworkParams::new().with_*`, and `apply_custom_qdisc`).
- Error types and architecture sections mention netlink-based behavior which is not reflected in the code.

4) Error handling and diagnostics
- Any non-zero exit from `tc` is mapped to `PermissionDenied` instead of parsing stderr and returning actionable errors (e.g., interface missing vs. insufficient privileges vs. invalid tc arguments).
- No verification step to confirm applied state (e.g., a follow-up `tc qdisc show dev <iface>` or parsing stats).

5) Interface validation
- No explicit check that the interface exists or is up before applying qdiscs; failures surface only indirectly through `tc`’s exit code.

6) Limited feature scope
- Only egress shaping on the device is targeted. Ingress simulation typically requires `ifb` redirection or shaping at the peer.
- No jitter support in `NetworkParams`; `NetemConfig` supports it but API doesn’t expose it.
- No way to specify reorder/duplicate from `NetworkParams`.

7) Test coverage and execution environment
- Integration tests depend on capabilities rarely present in CI; there’s no built-in skip/guard to mark them ignored unless `NET_ADMIN` is available. NOTE FROM USER: This is incorrect as the containerized environment used for testing has the necessary capabilities.
- No smoke test to assert that `tc` commands succeed and that applied qdiscs show up.

## Risks and edge cases
- Running without CAP_NET_ADMIN: commands fail; currently reported as `PermissionDenied` for many distinct root causes.
- Using loopback (`lo`) for multiple simulated paths: later applications override earlier ones; distinct veth pairs or namespaces are required for realistic multi-path simulations.
- Nonexistent or down interfaces: `tc` failures are not differentiated, hindering debuggability.
- Rate values: converting bps→kbit and chosen defaults for burst/latency might be inappropriate for some rates (e.g., very low/high extremes).

## Recommendations (prioritized)

Immediate (correctness and clarity)
1. Fix qdisc topology for rate + netem:
   - Option A (classful root):
     - `tc qdisc add dev IF root handle 1: htb default 10`
     - `tc class add dev IF parent 1: classid 1:10 htb rate <RATE> ceil <RATE>`
     - `tc qdisc add dev IF parent 1:10 handle 10: netem <delay/loss/...>`
   - Option B (netem-only with built-in rate if kernel supports): add `rate` in netem config directly; otherwise prefer HTB approach above.
   - Remove the attempt to attach netem under TBF.
2. Improve error mapping:
   - Capture and surface `tc` stderr; classify common cases: missing interface, insufficient permissions, invalid args.
   - Return distinct `QdiscError` variants accordingly.
3. Validate interface presence before applying:
   - Use `ip link show <iface>` or `tc qdisc show` pre-check; fail early with `InterfaceNotFound`.

Short-term (observability and API fidelity)
4. Implement `get_interface_stats` and `describe_interface_qdisc`:
   - Parse `tc -s qdisc show dev <iface>` to return basic stats (drops, bytes, packets) and current qdisc tree.
5. Align README with reality:
   - Document tc-based implementation (or switch to netlink and actually use the existing deps).
   - Remove or mark “future” APIs (builders, advanced qdisc config) until implemented.
6. Expose additional knobs in `NetworkParams`:
   - Optional `jitter_ms`, `reorder_pct`, `duplicate_pct`, and `loss_correlation_pct` passed through to netem.

Medium-term (robustness and UX)
7. Ingress simulation support:
   - Provide helpers to setup `ifb` and redirect ingress for realistic inbound impairment simulation.
8. Capability detection and test gating:
   - Add a helper `has_net_admin()` and mark integration tests as ignored unless capabilities are present.
9. Optional netlink backend:
   - Either remove the netlink dependencies or add a feature-flagged netlink backend with parity to tc backend. Choose one default and document trade-offs.

## Acceptance criteria for “ready”
- Applying typical/poor/good profiles works and survives a validate step that checks `tc qdisc show dev <iface>`.
- Rate + netem scenarios apply reliably using a classful root (HTB) or supported built-in netem rate.
- README examples compile and run (no missing APIs), or missing APIs are clearly marked as future work.
- Basic stats retrieval available for verification in tests.
- CI can run a smoke test gated by capability checks.

## Quick verification plan
- Local privileged run: apply `NetworkParams::typical()` to a disposable veth and confirm:
  - `tc qdisc show dev vethX` shows HTB root, class 1:10, netem child.
  - `tc -s qdisc show dev vethX` returns coherent counters.
- Negative tests:
  - Apply on missing interface → `InterfaceNotFound`.
  - Run without NET_ADMIN → `PermissionDenied` with clear stderr.

## Appendix: current implementation snapshot
- API:
  - `apply_network_params(&QdiscManager, iface, &NetworkParams)` – applies netem; adds TBF for rate
  - `remove_network_params(&QdiscManager, iface)` – deletes root qdisc
- Implementation: tc-based via `tokio::process::Command` (not netlink)
- Structs:
  - `NetworkParams { delay_ms, loss_pct (0.0–1.0), rate_kbps }`
  - `NetemConfig { delay_us, jitter_us, loss_percent, loss_correlation, reorder_percent, duplicate_percent, rate_bps }`
- Dependencies: `netlink-packet-route`, `netlink-sys` present but unused by current code

---
Prepared by: repository audit, 2025-09-02
