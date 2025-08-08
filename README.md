# RISTSmart Plugin

This repository contains two GStreamer elements that help with Reliable Internet Stream Transport (RIST) bonding and rate control:

* **`ristdispatcher`** — a lightweight, chain‑based dispatcher that fan‑outs an RTP stream to multiple per‑link branches and routes buffers using **manual per‑link weights**.
* **`dynbitrate`** — a simple bitrate controller intended to adjust an encoder’s `bitrate` property over time (currently a scaffold with basic behaviour; see *Limitations*).

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

Ensure the compiled shared library is on GStreamer’s plugin path. For local testing:

```bash
export GST_PLUGIN_PATH="$(pwd)/target/release"
# Verify:
gst-inspect-1.0 ristsmart
```

---

## 2) Element: `ristdispatcher`

**Category:** Filter/Network
**Pads:**

* **sink** (Always): `application/x-rtp`
* **src\_%u** (Request): `application/x-rtp`

`ristdispatcher` is designed to be connected upstream of `ristsink`. The RIST sink will **request one `src_%u` pad per bonded link** and link each branch to its own `queue ! ristrtxsend ! ...` chain.

### 2.1 Behaviour

* **Routing:** For every incoming RTP buffer on the **sink** pad, the dispatcher selects one `src_%u` according to a **weight vector** and pushes the buffer to that pad.
* **Selection algorithm:** Deterministic **weighted pick** with a tiny rotation penalty to avoid always choosing the same index on ties.
* **Manual weights:** You can set an initial weight vector (JSON) that biases traffic per link.

> ⚠️ **Runtime pad changes:** The current implementation assumes the set of bonded links is **stable while PLAYING**. Adding/removing RIST links after negotiation may require re‑(PAUSED→PLAYING) or upstream renegotiation.

### 2.2 Properties

| Name                    | Type                          | Default  | Description                                                                  |
| ----------------------- | ----------------------------- | -------- | ---------------------------------------------------------------------------- |
| `weights`               | `string` (JSON array)         | `[1.0]`  | Initial per‑link weights. Example: `[2.0,1.0,1.0]` biases the first link 2×. |
| `rebalance-interval-ms` | `uint64`                      | `500`    | Reserved for future dynamic weight recomputation. Not used in this build.    |
| `strategy`              | `string` (`"ewma"`\|`"aimd"`) | `"ewma"` | Reserved for future recompute strategy. Not used in this build.              |

### 2.3 Example: programmatic setup (Rust)

```rust
use gstreamer as gst;
use gst::prelude::*;

gst::init().unwrap();

let pipeline = gst::Pipeline::new();
let pay = gst::ElementFactory::make("rtph264pay").property("pt", 96).build().unwrap();
let ext = gst::ElementFactory::make("ristrtpext").build().unwrap();
let sink = gst::ElementFactory::make("ristsink").build().unwrap();
let disp = gst::ElementFactory::make("ristdispatcher").build().unwrap();

disp.set_property("weights", Some("[2.0,1.0]")); // bias link #0

// Attach dispatcher to ristsink (object-typed property → must be set in code)
sink.set_property("dispatcher", &disp);
// Configure bonded endpoints on ristsink (two links shown)
sink.set_property("bonding-addresses", &"10.0.0.1:5004,11.0.0.1:5006");

pipeline.add_many([&pay, &ext, &disp, &sink]).unwrap();
gst::Element::link_many([&pay, &ext, &disp, &sink]).unwrap();

pipeline.set_state(gst::State::Playing).unwrap();
```

> **Note:** The `dispatcher` property is a **GObject** (element) reference. It cannot be set from `gst-launch-1.0`; set it programmatically as above.

### 2.4 Example topologies

* **2‑link bonding with manual bias**

```
... ! rtph264pay pt=96 ! ristrtpext ! ristdispatcher weights="[3.0,1.0]" ! ristsink bonding-addresses=ipA:portA,ipB:portB
```

(Weights shown inline for illustration; set via code.)

* **1→N split before per‑link senders**

```
RTP → ristrtpext → ristdispatcher → src_0 → queue → ristrtxsend → ...
                              ↘ src_1 → queue → ristrtxsend → ...
```

### 2.5 Notes & caveats

* **Events/queries:** This build’s src‑pad event/query handlers are minimal. Plan pipelines so negotiation completes **before** PLAYING and avoid hot‑adding links.
* **Caps:** Both sink and src pads use `application/x-rtp`.
* **Error handling:** If the chosen `src_%u` is missing, the element reports an error (no automatic fallback chain).

---

## 3) Element: `dynbitrate`

**Category:** Filter/Network
**Pads:**

* **sink** (Always): `ANY`
* **src** (Always): `ANY`

`dynbitrate` is a control element meant to sit **beside** your main media chain. It periodically inspects a configured RIST sink and adjusts an encoder’s `bitrate` property in kilobits per second.

### 3.1 Properties

| Name                | Type         | Default | Description                                                           |
| ------------------- | ------------ | ------- | --------------------------------------------------------------------- |
| `encoder`           | `GstElement` | `NULL`  | The encoder whose `bitrate` (kbps) will be adjusted (e.g. `x264enc`). |
| `rist`              | `GstElement` | `NULL`  | The `ristsink` to read stats from (scaffold).                         |
| `min-kbps`          | `uint`       | `500`   | Minimum bitrate.                                                      |
| `max-kbps`          | `uint`       | `8000`  | Maximum bitrate.                                                      |
| `step-kbps`         | `uint`       | `250`   | Adjustment step per tick.                                             |
| `target-loss-pct`   | `double`     | `0.5`   | Target packet loss percentage (reserved).                             |
| `min-rtx-rtt-ms`    | `uint64`     | `40`    | Minimum retransmission RTT (reserved).                                |
| `downscale-keyunit` | `bool`       | `true`  | (Reserved) Force keyframe on downscale.                               |

### 3.2 Current behaviour

* A 500 ms timer calls `tick()`.
* If `encoder` is set and exposes a `bitrate` property (in **kbps**), the element performs a simple **guarded oscillation** between `min-kbps` and `max-kbps` using `step-kbps`.
* Hooks are present to read `rist` statistics (`rist/x-sender-stats` struct) but **parsing/adaptation logic is not implemented in this build**.

### 3.3 Example: programmatic setup (Rust)

```rust
let ctrl = gst::ElementFactory::make("dynbitrate").build().unwrap();
ctrl.set_property("encoder", &encoder_element);
ctrl.set_property("rist", &ristsink);
ctrl.set_property("min-kbps", 1500u32);
ctrl.set_property("max-kbps", 8000u32);
ctrl.set_property("step-kbps", 500u32);
```

> **Encoders:** `x264enc` uses `bitrate` in **kbps**. Other encoders may use different units/property names; adapt accordingly.

---

## 4) End‑to‑end examples

### 4.1 Two‑link bonding, manual bias, x264 encoding

```bash
# Pseudocode-ish: dispatcher must be set from an app, not from gst-launch
appsrc is-live=true ! videoconvert ! x264enc bitrate=4000 tune=zerolatency ! \
  rtph264pay pt=96 ! ristrtpext ! ristdispatcher ! ristsink \
  bonding-addresses="10.0.0.1:5004,11.0.0.1:5006"
# In code:
// disp.set_property("weights", Some("[3.0,1.0]"));
// sink.set_property("dispatcher", &disp);
```

### 4.2 Add `dynbitrate` control

Place `dynbitrate` anywhere in the pipeline graph (it has pass‑through pads) or keep it unattached and only use it as a controller.

```rust
pipeline.add_many([&ctrl]).unwrap();
ctrl.set_property("encoder", &x264enc);
ctrl.set_property("rist", &ristsink);
ctrl.set_property("min-kbps", 2500u32);
ctrl.set_property("max-kbps", 9000u32);
ctrl.set_property("step-kbps", 500u32);
```

---

## 5) Troubleshooting

* **“dispatcher” cannot be set from gst‑launch:** Correct — it’s an object‑typed property. Set it in code.
* **No traffic on one link:** Check `weights` JSON length: it should be ≥ the number of requested `src_%u` pads (i.e., links). Missing entries default to `1.0` during validation.
* **Caps negotiation issues:** Ensure RTP caps are set **before** PLAYING. This build does not replay sticky events to new pads.
* **Encoder bitrate has no effect:** Verify the encoder element actually has a `bitrate` (kbps) property. For non‑x264 encoders, property names/units differ.
* **Debugging:**

  ```bash
  GST_DEBUG=ristdispatcher:6,dynbitrate:5,ristsink:5,*:2 your-app
  ```

---

## 6) Performance & Tuning Tips

* **Weights:** Start with equal weights `[1,1,...]`, then bias toward the lower‑loss/lower‑RTT path.
* **Queueing:** Keep per‑link `queue` elements with enough buffers to absorb jitter.
* **RTT/Loss feedback:** For future dynamic balancing, plan to parse `ristsink`’s stats and maintain an EWMA of goodput and RTX rate per link.

---

## 7) Limitations (this build)

* `ristdispatcher` currently uses **manual** weights only. `rebalance-interval-ms` and `strategy` are reserved for future use.
* Minimal event/query handling on pads → avoid hot‑adding/removing links while PLAYING.
* `dynbitrate` does **not** yet adapt based on real `ristsink` statistics; it performs a bounded oscillation as a placeholder.

---

## 8) Roadmap (suggested)

* **Dispatcher pad semantics:** upstream event/query forwarding and sticky event replay to support hot‑plugging links mid‑stream.
* **Dynamic weights:** periodic recompute via EWMA/AIMD using per‑link stats sourced from `ristsink`.
* **`dynbitrate` integration:** parse `rist/x-sender-stats` and coordinate decisions with the dispatcher (single source of truth for link quality).
* **Signals/metrics:** expose `weights-changed`, and counters (per‑link dispatched packets, loss, RTT EWMA) for observability.

---

## 9) API Quick Reference

### `ristdispatcher`

* **Factory name:** `ristdispatcher`
* **Pads:** `sink` (Always, `application/x-rtp`), `src_%u` (Request, `application/x-rtp`)
* **Key properties:**

  * `weights` (string JSON) → `[f64; N]`
  * `rebalance-interval-ms` (u64) → reserved
  * `strategy` (string: `ewma|aimd`) → reserved

### `dynbitrate`

* **Factory name:** `dynbitrate`
* **Pads:** `sink` (Always, ANY), `src` (Always, ANY)
* **Key properties:** `encoder` (GstElement), `rist` (GstElement), `min-kbps`, `max-kbps`, `step-kbps`, `target-loss-pct`, `min-rtx-rtt-ms`, `downscale-keyunit`

---

**License:** MIT (per `lib.rs` plugin registration)

**Contact:** Maintainer field is set to “Jake” in element metadata.
