# RISTSmart Plugin

This repository contains two GStreamer elements that provide intelligent RIST bonding and adaptive rate control:

* **`ristdispatcher`** — an advanced, load‑balancing dispatcher that distributes RTP streams across multiple bonded links using **automatic weight adjustment** based on real‑time network statistics (loss, RTT, goodput). Supports both EWMA and AIMD strategies.
* **`dynbitrate`** — an intelligent bitrate controller that monitors RIST link performance and dynamically adjusts encoder bitrate based on actual network conditions. Can coordinate with dispatcher for unified adaptive control.

> Plugin ID: **`ristsmart`** (see `lib.rs`)
>
> Elements provided: `ristdispatcher`, `dynbitrate`

---

## 1) Build & Install

### Prerequisites

* GStreamer 1.20+ headers and runtime
* Rust 1.75+ and Cargo
* `gstreamer`/`glib` Rust crates (managed via Cargo)

### Build

```bash
cargo build --release
```

### Registering the plugin

Ensure the compiled shared library is on GStreamer's plugin path. For local testing:

```bash
export GST_PLUGIN_PATH="$(pwd)/target/release"
# Verify:
gst-inspect-1.0 ristsmart
```

---

## 2) Element: `ristdispatcher`

**Category:** Filter/Network
**Pads:**

* **sink** (Always): `ANY` (actual caps depend on upstream)
* **src\_%u** (Request): `application/x-rtp` (or `ANY` if `caps-any=true`)

`ristdispatcher` is designed to be connected upstream of `ristsink`. The RIST sink will **request one `src_%u` pad per bonded link** and link each branch to its own `queue ! ristrtxsend ! ...` chain.

### 2.1 Behaviour

* **Routing:** For every incoming RTP buffer on the **sink** pad, the dispatcher selects one `src_%u` according to a **weight vector** and pushes the buffer to that pad.
* **Selection algorithm:** Deterministic **weighted pick** with a tiny rotation penalty to avoid always choosing the same index on ties.
* **Automatic adaptation:** Real-time weight adjustment based on RIST statistics using EWMA or AIMD strategies.
* **Manual override:** You can set initial weight vectors (JSON) or disable auto-balance for static configurations.

> ⚠️ **Runtime pad changes:** The current implementation assumes the set of bonded links is **stable while PLAYING**. Adding/removing RIST links after negotiation may require re‑(PAUSED→PLAYING) or upstream renegotiation.

### 2.2 Properties

| Name                    | Type                          | Default  | Description                                                                                    |
| ----------------------- | ----------------------------- | -------- | ---------------------------------------------------------------------------------------------- |
| `weights`               | `string` (JSON array)         | `[1.0]`  | Initial per‑link weights. Example: `[2.0,1.0,1.0]` biases the first link 2×.                 |
| `rebalance-interval-ms` | `uint64`                      | `500`    | How often to recompute weights from statistics in milliseconds (range: 100-10000).           |
| `strategy`              | `string` (`"ewma"`\|`"aimd"`) | `"ewma"` | Strategy for automatic weight updates: `"ewma"` (goodput-based) or `"aimd"` (TCP-like).      |
| `caps-any`              | `boolean`                     | `false`  | Use ANY caps instead of application/x-rtp for broader compatibility.                          |
| `auto-balance`          | `boolean`                     | `true`   | Enable automatic rebalancing timer (disable when using external controller like dynbitrate). |
| `rist`                  | `GstElement`                  | `NULL`   | The RIST sink element to read statistics from for adaptive weighting.                         |
| `current-weights`       | `string` (JSON array, readonly) | `[1.0]`  | Current weight values as JSON array - readonly for monitoring.                                |

### 2.3 Signals

| Name              | Parameters                  | Description                                                    |
| ----------------- | --------------------------- | -------------------------------------------------------------- |
| `weights-changed` | `weights` (string)          | Emitted when automatic weight adjustment changes the weights. The `weights` parameter contains the new weights as a JSON array string. |

### 2.4 Example: programmatic setup (Rust)

```rust
use gstreamer as gst;
use gst::prelude::*;

gst::init().unwrap();

let pipeline = gst::Pipeline::new();
let pay = gst::ElementFactory::make("rtph264pay").property("pt", 96).build().unwrap();
let ext = gst::ElementFactory::make("ristrtpext").build().unwrap();
let sink = gst::ElementFactory::make("ristsink").build().unwrap();
let disp = gst::ElementFactory::make("ristdispatcher").build().unwrap();

// Configure automatic weight balancing
disp.set_property("auto-balance", true);
disp.set_property("strategy", "ewma");
disp.set_property("rebalance-interval-ms", 500u64);
disp.set_property("rist", &sink); // Link to RIST sink for statistics

// Or use manual weights (disables auto-balance)
// disp.set_property("weights", Some("[2.0,1.0]"));

// Attach dispatcher to ristsink (object-typed property → must be set in code)
sink.set_property("dispatcher", &disp);
// Configure bonded endpoints on ristsink (two links shown)
sink.set_property("bonding-addresses", &"10.0.0.1:5004,11.0.0.1:5006");

pipeline.add_many([&pay, &ext, &disp, &sink]).unwrap();
gst::Element::link_many([&pay, &ext, &disp, &sink]).unwrap();

pipeline.set_state(gst::State::Playing).unwrap();
```

> **Note:** The `dispatcher` property is a **GObject** (element) reference. It cannot be set from `gst-launch-1.0`; set it programmatically as above.

### 2.5 Example topologies

* **2‑link automatic bonding**

```
... ! rtph264pay pt=96 ! ristrtpext ! ristdispatcher auto-balance=true ! ristsink bonding-addresses=ipA:portA,ipB:portB
```

* **1→N split before per‑link senders**

```
RTP → ristrtpext → ristdispatcher → src_0 → queue → ristrtxsend → ...
                              ↘ src_1 → queue → ristrtxsend → ...
```

### 2.6 Notes & caveats

* **Events/queries:** This build's src‑pad event/query handlers are minimal. Plan pipelines so negotiation completes **before** PLAYING and avoid hot‑adding links.
* **Caps:** Sink pad uses ANY caps. Src pads use `application/x-rtp` by default, or ANY if `caps-any=true`.
* **Error handling:** If the chosen `src_%u` is missing, the element reports an error (no automatic fallback chain).

---

## 3) Element: `dynbitrate`

**Category:** Filter/Network
**Pads:**

* **sink** (Always): `ANY`
* **src** (Always): `ANY`

`dynbitrate` is a control element meant to sit **beside** your main media chain. It periodically inspects a configured RIST sink and adjusts an encoder's `bitrate` property in kilobits per second.

### 3.1 Properties

| Name                | Type         | Default | Description                                                                                           |
| ------------------- | ------------ | ------- | ----------------------------------------------------------------------------------------------------- |
| `encoder`           | `GstElement` | `NULL`  | The encoder whose `bitrate` (kbps) will be adjusted (e.g. `x264enc`).                               |
| `rist`              | `GstElement` | `NULL`  | The `ristsink` to read stats from for intelligent bitrate adjustment.                                |
| `min-kbps`          | `uint`       | `500`   | Minimum bitrate in kilobits per second (range: 100-100000).                                         |
| `max-kbps`          | `uint`       | `8000`  | Maximum bitrate in kilobits per second (range: 500-100000).                                         |
| `step-kbps`         | `uint`       | `250`   | Adjustment step per tick in kilobits per second (range: 50-5000).                                   |
| `target-loss-pct`   | `double`     | `0.5`   | Target packet loss percentage for bitrate adjustment (range: 0.0-10.0).                            |
| `min-rtx-rtt-ms`    | `uint64`     | `40`    | RTT floor threshold in milliseconds - higher RTT triggers bitrate reduction (range: 10-1000).      |
| `dispatcher`        | `GstElement` | `NULL`  | The RIST dispatcher element to coordinate with for unified control.                                  |

### 3.2 Current behaviour

* A 750 ms timer calls `tick()` to continuously monitor and adjust (offset from dispatcher to avoid conflicts).
* **Intelligent adaptation:** When `rist` element is configured, reads real statistics (`rist/x-sender-stats` struct) including:
  - **Loss rate** from retransmission statistics  
  - **RTT measurements** for latency monitoring
  - **Per-session data** for multi-link bonding scenarios
* **Bitrate adjustment logic:** 
  - **Decreases** bitrate when loss > `target-loss-pct` OR RTT > `min-rtx-rtt-ms`
  - **Increases** bitrate when loss < 50% of target AND RTT < 80% of threshold
  - Respects `min-kbps`, `max-kbps`, and `step-kbps` boundaries
  - Rate limiting prevents rapid changes (minimum 1200ms between adjustments)
* **Dispatcher integration:** Can compute and set dispatcher weights when `dispatcher` property is configured
* **Conflict prevention:** Automatically disables dispatcher auto-balance when connected to prevent dueling controllers
* **Fallback mode:** Performs bounded oscillation between min/max when no RIST statistics available

### 3.3 Example: programmatic setup (Rust)

```rust
let ctrl = gst::ElementFactory::make("dynbitrate").build().unwrap();
ctrl.set_property("encoder", &encoder_element);
ctrl.set_property("rist", &ristsink);
ctrl.set_property("dispatcher", &dispatcher); // Enable unified control
ctrl.set_property("min-kbps", 1500u32);
ctrl.set_property("max-kbps", 8000u32);
ctrl.set_property("step-kbps", 500u32);
ctrl.set_property("target-loss-pct", 1.0); // 1% target loss
ctrl.set_property("min-rtx-rtt-ms", 60u64); // 60ms RTT threshold
```

> **Encoders:** `x264enc` uses `bitrate` in **kbps**. Other encoders may use different units/property names; adapt accordingly.

---

## 4) End‑to‑end examples

### 4.1 Two‑link bonding with automatic adaptation

```bash
# Pseudocode-ish: dispatcher must be set from an app, not from gst-launch
appsrc is-live=true ! videoconvert ! x264enc bitrate=4000 tune=zerolatency ! \
  rtph264pay pt=96 ! ristrtpext ! ristdispatcher ! ristsink \
  bonding-addresses="10.0.0.1:5004,11.0.0.1:5006"
# In code:
// disp.set_property("auto-balance", true);
// disp.set_property("rist", &sink);
// sink.set_property("dispatcher", &disp);
```

### 4.2 Add `dynbitrate` unified control

Place `dynbitrate` anywhere in the pipeline graph (it has pass‑through pads) or keep it unattached and only use it as a controller.

```rust
pipeline.add_many([&ctrl]).unwrap();
ctrl.set_property("encoder", &x264enc);
ctrl.set_property("rist", &ristsink);
ctrl.set_property("dispatcher", &disp); // Unified control
ctrl.set_property("min-kbps", 2500u32);
ctrl.set_property("max-kbps", 9000u32);
ctrl.set_property("step-kbps", 500u32);
```

---

## 5) Troubleshooting

* **"dispatcher" cannot be set from gst‑launch:** Correct — it's an object‑typed property. Set it in code.
* **No traffic on one link:** Check `weights` JSON length: it should be ≥ the number of requested `src_%u` pads (i.e., links). Missing entries default to `1.0` during validation.
* **Caps negotiation issues:** Ensure caps are set **before** PLAYING. Consider using `caps-any=true` for broader compatibility.
* **Encoder bitrate has no effect:** Verify the encoder element actually has a `bitrate` (kbps) property. For non‑x264 encoders, property names/units differ.
* **Weights not updating automatically:** Check that `auto-balance=true`, `rist` element is set, and RIST statistics are available.
* **DynBitrate conflicts:** If using both DynBitrate and dispatcher auto-balance, DynBitrate will automatically disable dispatcher auto-balance.
* **Debugging:**

  ```bash
  GST_DEBUG=ristdispatcher:6,dynbitrate:5,ristsink:5,*:2 your-app
  ```

---

## 6) Performance & Tuning Tips

* **Automatic vs Manual Control:** 
  - **Enable `auto-balance=true`** on dispatcher for automatic weight adjustment based on RIST statistics
  - **Use manual weights** `[1,1,...]` only for static scenarios or initial setup
  - **Connect DynBitrate** to both dispatcher and encoder for coordinated adaptive control

* **Load Balancing Strategies:**
  - **EWMA strategy** (default): Goodput-based weighting with loss and RTT penalties - best for varying conditions
  - **AIMD strategy**: TCP-like fairness with additive increase/multiplicative decrease - best for congestion-prone networks

* **Rebalance Timing:** 
  - Default `rebalance-interval-ms=500` works well for most cases
  - Reduce to 200-300ms for rapid network changes  
  - Increase to 1000ms for stable links to reduce overhead

* **DynBitrate Coordination:**
  - Set `dispatcher` property on DynBitrate to enable unified control
  - DynBitrate automatically disables dispatcher auto-balance to prevent conflicts
  - Monitor `target-loss-pct` and `min-rtx-rtt-ms` thresholds for your network conditions

* **Monitoring:** 
  - Use `current-weights` property to monitor real-time weight adjustments
  - Listen to `weights-changed` signal for programmatic weight change notifications
  - Enable debug logging: `GST_DEBUG=ristdispatcher:6,dynbitrate:5`

* **Network Optimization:**
  - **Queueing:** Keep per‑link `queue` elements with enough buffers to absorb jitter
  - **Initial bias:** Start with equal weights, let automatic adjustment optimize over time
  - **Statistics quality:** Ensure RIST sink provides detailed per-session statistics for best adaptation

---

## 7) Current Limitations

* **Event/query handling:** Minimal event/query handling on src pads → avoid hot‑adding/removing links while PLAYING. Plan pipelines so negotiation completes **before** PLAYING.
* **Pad template flexibility:** While `caps-any` property allows ANY caps, the default templates expect `application/x-rtp`.
* **Statistics dependency:** Automatic weight adjustment requires RIST sink statistics. Without stats, DynBitrate falls back to oscillation mode.
* **Controller coordination:** When using both dispatcher auto-balance and DynBitrate simultaneously, DynBitrate automatically disables dispatcher auto-balance to prevent conflicts.

---

## 8) Roadmap (suggested)

**Completed Features:**
* ✅ **Dynamic weights:** Periodic recompute via EWMA/AIMD using per‑link stats from `ristsink`
* ✅ **`dynbitrate` integration:** Full parsing of `rist/x-sender-stats` with coordinated dispatcher weight control
* ✅ **Signals/metrics:** `weights-changed` signal, `current-weights` property for real-time monitoring

**Future Improvements:**
* **Enhanced pad semantics:** Better upstream event/query forwarding and sticky event replay to support hot‑plugging links mid‑stream
* **Advanced statistics:** More granular per-link counters (dispatched packets, bandwidth utilization, jitter measurements)
* **Performance optimization:** Lock contention reduction, batched weight updates for high-throughput scenarios  
* **Configuration presets:** Pre-tuned strategy/threshold combinations for common network scenarios

---

## 9) API Quick Reference

### `ristdispatcher`

* **Factory name:** `ristdispatcher`
* **Pads:** `sink` (Always, ANY caps), `src_%u` (Request, `application/x-rtp` or ANY)
* **Key properties:**
  * `weights` (string JSON) → `[f64; N]` initial weights
  * `rebalance-interval-ms` (u64) → rebalance timer interval  
  * `strategy` (string: `ewma|aimd`) → automatic weight adjustment strategy
  * `auto-balance` (bool) → enable automatic rebalancing
  * `caps-any` (bool) → use ANY caps instead of application/x-rtp
  * `rist` (GstElement) → RIST sink for statistics
  * `current-weights` (string JSON, readonly) → current weights for monitoring
* **Signals:**
  * `weights-changed` (weights: string) → emitted on automatic weight updates

### `dynbitrate`

* **Factory name:** `dynbitrate`
* **Pads:** `sink` (Always, ANY), `src` (Always, ANY)
* **Key properties:** `encoder` (GstElement), `rist` (GstElement), `dispatcher` (GstElement), `min-kbps`, `max-kbps`, `step-kbps`, `target-loss-pct`, `min-rtx-rtt-ms`

---

**License:** MIT (per `lib.rs` plugin registration)

**Contact:** Maintainer field is set to "Jake" in element metadata.
