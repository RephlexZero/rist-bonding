# RIST Single‑Stream Dispatcher + Dynamic Bitrate (Rust/GStreamer)

This document updates the earlier review for the **single‑stream** case: one encoded video stream (RTP) dispatched across multiple WAN links for **RIST** bonding. It summarizes what the plugin already does, flags gaps, and gives concrete usage and tuning guidance.

> Plugin crate: **`ristsmart`** → Elements: **`ristdispatcher`**, **`dynbitrate`**
> (See `lib.rs`: plugin\_define name `ristsmart`; elements registered in `plugin_init`).

---

## 1) What the plugin already does (single‑stream)

### `ristdispatcher` (one sink → N request src pads)

* **Pads**

  * Sink: `sink` (Always). Caps template is `ANY`; real caps are governed by the `caps-any` property and downstream negotiation.
  * Src (Request): `src_%u` with two templates available:

    * `application/x-rtp` (preferred for RTP pipelines)
    * `ANY` (fallback/utility)
* **Core behavior**

  * Receives **one RTP stream** on `sink` and **chooses one output pad per buffer** using **weights** (see strategy below), pushing the buffer into the first **linked** pad by priority, with fallback to other linked pads if the chosen one is not linked.
  * **Weighted selection** avoids sticky choices via a tiny rotation penalty; selection is O(N) per buffer.
* **Adaptive weighting**

  * Polls a configured **`ristsink`** element’s `stats` property (a `GstStructure`) on a timer.
  * Updates runtime weights using **EWMA** (default) or **AIMD** strategies based on per‑session goodput, retransmission rate, and RTT.
  * Normalizes weights and emits a **`weights-changed`** signal; exposes **`current-weights`** (read‑only JSON).
* **Properties**

  * `weights` *(string, JSON)* – initial weights, e.g. `[1.0,1.0]`.
  * `rebalance-interval-ms` *(u64)* – e.g. 500 ms.
  * `strategy` *(string)* – `"ewma"` | `"aimd"`.
  * `caps-any` *(bool)* – if true, uses ANY caps on src pads (useful for non‑RTP testing).
  * `auto-balance` *(bool)* – run internal timer to poll stats & update weights automatically.
  * `rist` *(GstElement)* – reference to the **ristsink** whose stats are read.
  * `current-weights` *(string, READABLE)* – normalized weights JSON for monitoring.

### `dynbitrate` (controller for one encoder + dispatcher)

* **Pads**: passthrough `sink` → `src` (Always) for control‑element convenience; it does not transform payload.
* **Inputs/Outputs**

  * Reads `ristsink` **aggregate stats** (overall loss/RTX/RTT) and **per‑session stats** to derive **dispatcher weights**.
  * Adjusts a single **encoder** element’s `bitrate` property within `[min-kbps, max-kbps]` in steps of `step-kbps` with a **1.2 s cooldown** to avoid oscillations.
* **Properties**

  * `encoder` *(GstElement)* – the encoder to control (e.g., `x264enc`, `x265enc`, `vtenc_h264`, etc.).
  * `rist` *(GstElement)* – the ristsink providing stats.
  * `dispatcher` *(GstElement)* – so it can update `weights` coherently with rate changes.
  * `min-kbps`, `max-kbps`, `step-kbps` *(uints)*.
  * `target-loss-pct` *(double, e.g., 0.5 → 0.5%)* – aim to stay at/under this loss by backing off.
  * `min-rtx-rtt-ms` *(u64)* – RTT floor/guard in decisions.

---

## 2) Single‑stream implications (what matters & what to drop)

* **Per‑link QoS/ABR for multiple streams** is **out of scope**: you have *one* video stream. Keep **one encoder** and one RTP payloader.
* The **dispatcher’s job** is **per‑packet path selection** across sessions/links. That’s correct for single‑stream bonding.
* The **bitrate controller** should drive **one encoder** total‑rate; don’t try to maintain per‑link encodes.

---

## 3) Gaps & targeted recommendations (single‑stream)

1. **Selective duplication (optional, single‑stream‑friendly)**
   Add a boolean `duplicate-keyframes` with a per‑second budget to **duplicate only keyframe packets** (IDR / random access) onto the *second‑best* link when loss spikes or during failover windows.
   *Implementation hints*: detect keyframes via RTP payload/marker + `GST_BUFFER_FLAG_DELTA_UNIT==FALSE` when present; or allow an upstream tag/meta to mark “important” buffers.

2. **Hysteresis for link switching**
   Avoid rapid back‑and‑forth path flips that cause reorder at the receiver. Add:

   * `min-hold-ms` before changing the chosen pad when weights are close,
   * and/or a **stickiness factor** (bias to previous pad unless weight ratio > threshold).

3. **Warm‑up & health checks**
   Treat a newly linked pad as **probationary** until it shows sane RTT/RTX for a short window; otherwise avoid immediately routing key GOPs over it.

4. **Graceful failover**
   On hard degradation (high RTX, rising RTT) switch paths **between GOPs** when possible; if you can’t detect GOPs, switch at **RTP marker boundaries** to reduce receiver jitter.

5. **Metrics/observability**
   Export a small JSON or Prometheus‑style metric set: selected pad index, per‑link EWMA goodput/rtx/rtt, weight vector, and encoder bitrate. You already expose `current-weights`; add counts and last‑switch timestamp.

6. **Ingress buffering**
   `ristdispatcher` is chain‑based and forwards immediately. That’s fine for low‑latency, but consider a small **jitter buffer** (a few ms) if you notice ordering artifacts during path swaps.

7. **Caps handling**
   Keep `application/x-rtp` as the default for src pads in production; `caps-any` is useful only for synthetic tests.

---

## 4) How the strategies map to single‑stream

* **EWMA (default)**: Good for steady sharing—weights reflect sustained per‑link goodput and penalize RTX/RTT. In single‑stream, it turns into “send most packets down the most reliable/fast link, bleed a bit to others if warranted”.
* **AIMD**: Good for probing—will periodically increase weight to underused links and back off on loss. In single‑stream, it is a simple way to **probe alternates** without separate test traffic.

**Tip**: Use **EWMA** for production; toggle to **AIMD** during testing to verify that alternates remain usable.

---

## 5) Example pipelines (single stream)

> **Note**: Exact pad names of your `ristsink` may vary. The general pattern is one RTP payloader → `ristdispatcher` → multiple requested src pads linked to the RIST outputs/sessions.

### Encoding + RTP + Dispatch + RIST (conceptual)

```bash
# Encode + packetize
videotestsrc is-live=true ! timeoverlay ! x264enc tune=zerolatency bitrate=2500 ! \
  rtph264pay config-interval=1 pt=96 ! \
  ristsmart ristdispatcher name=disp strategy=ewma rebalance-interval-ms=500 auto-balance=true ! \
  ristsink name=rs # (configure rs to create sessions/links, then link rs to disp.src_%u pads)
```

Then, from your app or launch file, request and **link multiple `disp.src_%u` pads** to the appropriate RIST session sinks (or configure your `ristsink` helper to request them automatically from the upstream `disp`).

### Bitrate controller

```bash
# dynbitrate attached out-of-band to the pipeline
ristsmart dynbitrate name=ctrl \
  encoder=x264enc0 rist=rs dispatcher=disp \
  min-kbps=800 max-kbps=3500 step-kbps=200 target-loss-pct=0.5 min-rtx-rtt-ms=40
```

Place `dynbitrate` logically near the encoder (it’s a pass‑through). The element will poll stats every \~750 ms (offset from dispatcher) and adjust the encoder and dispatcher weights together.

---

## 6) Tuning checklist (single‑stream)

* **Encoder**: enable low‑latency mode (no B‑frames, small `key-int-max`), periodic IDR.
* **Dispatcher**:

  * `strategy=ewma`, `rebalance-interval-ms=500` (start here).
  * Initialize `weights` to prefer your primary link, e.g., `[2.0, 1.0, 1.0]`.
  * Consider adding `min-hold-ms=300` (see Recommendation #2) to reduce thrash.
* **DynBitrate**:

  * `min-kbps`/`max-kbps` to span your expected total bonded capacity.
  * `step-kbps=150–300`, `target-loss-pct≈0.5%`, `min-rtx-rtt-ms≈40–60`.
* **Test rig**: combine **OU‑driven rate** + **Gilbert–Elliott loss** per link to validate switching and bitrate reactions.

---

## 7) Potential pitfalls (and how your code addresses them)

* **No linked pads** → returns `FlowError::NotLinked` (already logged). Ensure at least one pad is linked before starting playback.
* **Query/event forwarding**: sink forwards to the first linked src pad; this is correct for most queries (allocation/latency), but monitor if any downstream element expects per‑pad responses.
* **Weight normalization**: your code normalizes and thresholds change detection at \~1%—good for stability.
* **Cooldown for bitrate**: 1.2 s guard exists—keeps oscillations down.

---

## 8) Nice‑to‑have single‑stream features (backlog)

* **`duplicate-keyframes` + `dup-budget-pps`** properties.
* **`min-hold-ms` / `switch-threshold`** properties to control hysteresis.
* **`health-warmup-ms`** for newly linked pads.
* **Stats export** pad or bus message (JSON) for external observers.

---

## 9) Summary

With one video stream, your current split of responsibilities is sound:

* `ristdispatcher` does **per‑packet path selection** based on link quality (EWMA/AIMD from RIST stats).
* `dynbitrate` keeps **one encoder** in a safe operating window and helps the dispatcher weight links coherently.

Add **lightweight duplication**, **switch hysteresis**, and **metrics**, and you’ll have a production‑ready single‑stream bonding module that plays nicely with real‑world cellular variability.
