love it — let’s level up your testing and move it out of `src/` so you can hammer this plugin locally first, then push the same suites into CI/CD. Below is a concrete plan that folds in your original review ideas (EWMA/AIMD, hysteresis, keyframe duplication, metrics) and the network‑emulator testing you proposed (OU rate changes + Gilbert–Elliott burst loss) — all Rust‑only.&#x20;

# Goals

* Prove distribution correctness (SWRR ≈ requested weights), stability (hysteresis/warm‑up), and clean GStreamer pad semantics (caps, events, stickies).
* Prove control‑loop behavior (stats → weights → bitrate) using a **Rust mock** of `ristsink` stats that emits the real `rist/x-sender-stats` layout.
* Validate under **realistic cellular conditions** (OU‑driven throughput + GE burst loss) using a Rust netlink-based emulator — still Rust‑only, no shelling out.&#x20;
* Make everything runnable locally, then promote to CI (unprivileged first; privileged emulator tests on a self‑hosted or nightly job).

# Repository layout (workspace)

Turn your repo into a workspace and move tests out of `src/`:

```
.
├─ Cargo.toml                      # [workspace]
├─ ristsmart/                      # your plugin crate (no tests here except tiny unit tests)
│  ├─ Cargo.toml
│  └─ src/
│     ├─ lib.rs
│     ├─ dispatcher.rs
│     └─ dynbitrate.rs
├─ ristsmart-harness/              # tiny GST test elements + utilities
│  ├─ Cargo.toml
│  └─ src/
│     ├─ counter_sink.rs           # counts buffers per pad, asserts EOS/FLUSH
│     ├─ encoder_stub.rs           # exposes a "bitrate" property for dynbitrate tests
│     ├─ riststats_mock.rs         # emits rist/x-sender-stats with session-stats array
│     └─ bus_capture.rs            # captures JSON metrics bus messages
├─ ristsmart-tests/                # integration + element tests (no root required)
│  ├─ Cargo.toml
│  └─ tests/
│     ├─ unit_alg_swrrobin.rs      # pure SWRR + hysteresis unit/property tests
│     ├─ unit_ewma.rs              # EWMA/AIMD math on synthetic deltas
│     ├─ elem_pad_semantics.rs     # caps proxying, stickies replay, EOS/FLUSH fanout
│     ├─ elem_weighted_flow.rs     # appsrc → dispatcher → N counter_sinks (~weight splits)
│     ├─ stats_driven_logic.rs     # riststats_mock → dispatcher+dynbitrate end-to-end
│     └─ metrics_contract.rs       # validate JSON metrics schema/rate
├─ ristsmart-netem/                # privileged emulator & integr. tests (Linux only)
│  ├─ Cargo.toml
│  ├─ src/
│  │  └─ lib.rs                    # OU + netlink config apis, GE params, jitter
│  ├─ tests/
│  │  └─ integr_cellular.rs        # #[ignore] by default; enable via env/feature
│  └─ src/bin/emulator.rs          # local manual runs: spin namespaces & pipelines
├─ fixtures/                       # optional scenario JSONs for stats/emulator
└─ .github/workflows/ci.yml        # see “CI plan” below
```

**Why this split?**

* `ristsmart` stays lean; only a handful of tiny `#[cfg(test)]` unit tests remain.
* `ristsmart-harness` gives you **Rust-only** mock encoders/sinks and a **mock RIST stats element** that produce the exact `session-stats` array the dispatcher/dynbitrate expect.&#x20;
* `ristsmart-tests` holds most tests and depends on both `ristsmart` and `ristsmart-harness`.
* `ristsmart-netem` isolates privileged tests (Linux netns + tc via netlink). That’s your OU + GE testbed from the plan.&#x20;

# Test layers & what to assert

## 1) Pure algorithm tests (no GStreamer)

Location: `ristsmart-tests/tests/unit_*`

* **SWRR distribution**: for weights `[6,4]` over 100k packets, assert counts within ±0.5% of 60/40. Same for `[5,3,2]`.
* **Hysteresis & warm‑up**: drive weight sequences that hover near a boundary; assert **switches/sec** ≤ threshold with `min-hold-ms` and `switch-threshold`.
* **EWMA/AIMD math**: feed synthetic deltas of `sent-original`, `sent-retransmitted`, `round-trip-time`; verify normalized weights track expectations and damp oscillations.
* **Property tests (optional)**: use `proptest` to generate weight/time series and assert invariants like “no negative weights,” “no NaN,” “switches bounded by hold time.”

> These cover your review’s adaptive weighting goals without the complexity of pads/pipelines.&#x20;

## 2) Element‑level tests (pads, caps, events — still no real network)

Location: `ristsmart-tests/tests/elem_*` using `gst` + `glib` and `ristsmart-harness`.

* **Pad request & stickies replay**: start stream → request `src_0` later → assert STREAM\_START/CAPS/SEGMENT/TAG are replayed.
* **EOS/FLUSH fanout**: push EOS/FLUSH on sink, assert all linked counter sinks receive matching events.
* **Caps proxy**: with `caps-any=false`, assert sink pad caps mirror upstream RTP caps (use `PROXY_CAPS | PROXY_SCHEDULING` behavior you implemented).
* **Weighted flow**: `appsrc` (RTP) → `ristdispatcher` → `counter_sink`×N; set weights to `[0.7,0.3]` and assert buffer counts ≈ split.

## 3) Stats‑driven end‑to‑end (no root)

Location: `ristsmart-tests/tests/stats_driven_logic.rs`

* Build a pipeline with **mock stats** and **encoder stub** only:

  ```
  appsrc (RTP) → ristsmart::ristdispatcher (rist=riststats_mock0)
                 → counter_sink_0
                 → counter_sink_1
  dynbitrate (encoder=encoder_stub0, rist=riststats_mock0, dispatcher=dispatcher)
  ```
* Scenarios (scripted by the mock):

  * **Baseline**: both links healthy → near‑even split for equal weights.
  * **Degrade link B**: increased `sent-retransmitted` and rising RTT in session 1; assert weights shift toward A, bitrate backs off toward min until loss \~ target.
  * **Recovery**: return to healthy; assert bitrate rises (cooldown honored) and weights normalize.
  * **New link warm‑up**: add a third session; during `health-warmup-ms` it shouldn’t get key GOPs or primary traffic; after warm‑up it joins distribution.
  * **Keyframe duplication (if enabled)**: when loss spikes, only **key units** (non‑DELTA) are duplicated to the **best other** link; assert per‑pad counts reflect the budget.

> This concretely tests the single‑stream responsibilities you set in your review (dispatcher chooses path per packet; dynbitrate regulates the single encoder).&#x20;

## 4) Metrics contract tests

Location: `ristsmart-tests/tests/metrics_contract.rs`

* Subscribe to your bus messages (e.g., `"rist-dispatcher-metrics"`). Assert:

  * Schema fields exist: `{selected_idx, weights[], ewma_{goodput,rtx,rtt}[], encoder_bitrate, last_switch_ts}`.
  * Emission cadence within a tolerance (e.g., 900–1100 ms for 1s interval).
  * Values are sane (weights sum≈1, RTT>0 when sessions exist, etc.).
* These make CI failures actionable without staring at logs. Your review explicitly called for observability; these lock it in.&#x20;

## 5) Privileged integration (OU + GE cellular realism) — optional in CI

Location: `ristsmart-netem/tests/integr_cellular.rs` (marked `#[ignore]` by default; enable with `RISTS_PRIV=1` or a cargo feature)

* Use Rust netlink crates to create **one net namespace per link**, attach **TBF/CAKE** rate limiters, **netem** delay/jitter, and **GE loss**; update TBF with an **OU process** every 200 ms.&#x20;
* Full pipeline with real encoder or encoder stub. Assert:

  * **Failover**: during GE “bad” periods, active path switches within ≤1 GOP or an RTP marker interval (once you enforce marker‑aligned switches).&#x20;
  * **Loss target**: average post‑recovery loss ≈ target (0.5%) with dynbitrate’s dead‑band/cooldown respected.
  * **Throughput**: goodput tracks the current best link, and weights follow OU capacity changes.

# CI/CD plan

**Job A — Unprivileged (always on)**

* OS: Ubuntu LTS
* Deps: `libgstreamer1.0-dev`, base plugins
* Steps:

  * `cargo fmt -- --check`
  * `cargo clippy --all-targets --workspace -D warnings`
  * `cargo test -p ristsmart-tests -- --nocapture` (runs layers 1–4)

**Job B — Privileged (nightly or self‑hosted runner)**

* Requires `CAP_NET_ADMIN` (GitHub Actions can’t grant caps reliably without a privileged container or self‑hosted runner).
* Steps:

  * `cargo test -p ristsmart-netem -- --ignored --nocapture` (only when `RISTS_PRIV=1`).
* Artifacts: upload test logs and metrics JSON.

# Implementation notes & tiny snippets

## A) `riststats_mock` element (sketch)

* Element exposes a `stats` READABLE property returning a `gst::Structure` named `rist/x-sender-stats` with:

  * `session-stats`: `glib::ValueArray` of per‑session `gst::Structure`s containing `sent-original-packets`, `sent-retransmitted-packets`, `round-trip-time` (µs or ms — match your reader).
* Drive scenarios by setting a **TestController** on the element (channel/Arc) to update counters over time; emit `notify::stats` to wake pollers.

This mirrors the real C `ristsink` shape your Rust now expects, so you’re truly exercising the dispatcher/dynbitrate logic.&#x20;

## B) Counter sink

* Minimal sink that increments a per‑pad atomic and records last event (EOS/FLUSH). Optional: attach to bus for metrics.

## C) Determinism

* Seed the OU RNG and any random test inputs with a fixed seed.
* Use tolerances (ε) for float comparisons (weights, loss), and wall‑clock guard bands for timers (cooldown, rebalance).

# Migration steps (from “everything in src/” → the workspace)

1. Create the workspace `Cargo.toml` and sub‑crate folders.
2. Move your big tests out of `src/` into `ristsmart-tests/tests/`; keep tiny unit tests in `ristsmart/src/*` if they’re truly white‑box.
3. Add `ristsmart-harness` with the three helper elements/utilities.
4. Gate emulator tests with `#[ignore]` + env var/feature.
5. Add the CI workflow with Job A only; wire Job B later when you have a self‑hosted runner.

# Coverage you’ll achieve (mapped to the original plan)

* **Single‑stream design validation**: dispatcher = per‑packet path selection; dynbitrate = single encoder control — all proven with the stats‑mock pipelines.&#x20;
* **Backlog features** (keyframe duplication, hysteresis, warm‑up): behavior locked with unit + element tests, then revalidated under OU + GE.&#x20;
* **Realism**: OU‑driven rate & GE loss in namespaces reproduce cellular variability without leaving Rust.&#x20;

---

If you want, I can generate the initial three harness files (`riststats_mock.rs`, `counter_sink.rs`, `encoder_stub.rs`) with the correct `gst::subclass` boilerplate so you can paste them in and start writing tests immediately.
