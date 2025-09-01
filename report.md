# Test Audit Report (suspect passing tests)

Date: 2025-09-01

This report lists tests that currently pass but exhibit results or logs that may be misleading or insufficiently asserted, and proposes concrete fixes. It also notes the RTX-related handling enhancements.

## Summary of suspects

- Unit: `tests/unit/ewma_algorithm.rs` and `tests/unit/aimd_algorithm.rs`
  - Symptom: Logs show zero deltas during “adaptation” phases; tests pass without asserting redistribution.
  - Cause: Tests focus on property/config scaffolding, not traffic movement; short durations/samples.
  - Fix: Keep as configuration tests (clarified comments already added). Add a dedicated redistribution test in integration with synthetic stats or network-sim.

- Performance: `tests/performance_evaluation.rs`
  - Symptom: Degenerate distribution (single-path dominance) despite bonding; test still passes due to TS integrity OK.
  - Cause: Local env with equal links; actual distribution computed over cumulative snapshots previously; lack of real impairments.
  - Fix: Implemented per-interval deltas and dominance analysis; for stronger validity, wire network-sim/netem to induce asymmetric loss/RTT.

- Integration: `tests/integration/metrics_accuracy.rs` (legacy message type)
  - Symptom: Looks for `rist-dispatcher-stats` Element messages; current implementation emits Application messages `rist-dispatcher-metrics`.
  - Risk: Test may become vacuous (accepts empty) or drift from actual metrics.
  - Fix: Prefer `tests/integration/metrics_export.rs`, which validates current message format. Consider deprecating or aligning this file to Application messages.

- Integration: `tests/integration/pipeline_tests.rs` (dynamic pad addition/removal)
  - Symptom: Zero counters on late-added pad can look wrong; previously saw lifecycle warnings on removal.
  - Fix: Clarified expectations, extended runtime, and ensured NULL state before removal (warning eliminated).

## Implemented improvements (RTX/RTT)

- Dispatcher properties now expose:
  - `ewma-rtx-penalty` (alpha)
  - `ewma-rtt-penalty` (beta)
  - `aimd-rtx-threshold`
- Metrics messages include these tunables for observability.
- EWMA/AIMD calculations use the tunables directly.

## Recommended follow-ups

1) Add an integration test to validate RTX penalty effect
   - Set weights equal; feed synthetic per-session stats (or use network-sim) where link A has higher RTX% than B.
   - Assert B’s weight > A’s after adaptation; record in metrics.

2) Align or remove `metrics_accuracy.rs` Element-message tests
   - Update to observe Application messages `rist-dispatcher-metrics` for structural checks, or mark as deprecated.

3) Performance test with controlled impairments
   - Use `crates/network-sim` or netem to introduce per-link loss/RTT, confirm interval distributions follow configured strategy.

4) Strengthen “adaptation” unit tests
   - Keep the configuration/unit semantics, but add a small mock of link_stats to exercise EWMA weight changes deterministically.

---

If you want, I can proceed to add the integration test for RTX penalty and update `metrics_accuracy.rs` to align with the new Application metrics messages.
