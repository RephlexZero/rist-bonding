You’re not doing anything “wrong” in the Rust dispatcher so much as you’re asking it to do something it can’t do with the signals it has today.

**Root cause (why it won’t converge to link throughput):**

* Your **capacity estimate** uses the sender-side counter
  `ewma_goodput = Δ(sent-original-packets)/Δt` and then does
  `cap_est ≈ ewma_goodput / last_share`.
* But `sent-original-packets` is **what you decided to send**, not what the path could carry. If you give a link 30% of the traffic, its `sent-original` rate is \~30% of the source rate regardless of the path’s headroom. Dividing by 0.30 just gives you the **source rate** again. In other words, with only sender-side “originals”, **cap\_est is almost constant across links**, and the weights end up drifting on noise (RTT jitter, timing), not on true capacity.

That’s why you saw weird splits (and earlier, collapse): the estimator had no way to see *delivered* rate per path.

---

## What Kiloview / LiveU-like bonding actually optimizes

Vendors that “fill the pipe per link” do **two loops**:

1. **Capacity discovery loop (slow, explicit)**
   Continuously estimate **per-link usable throughput** (uplink capacity) from *receiver feedback* (ACK/RR) + active micro‑probes when total offered load is below the aggregate capacity.

2. **Traffic scheduler (fast, reactive)**
   Split traffic **proportional to the capacity estimates** (with a small explore floor), and apply short-timescale guards (back off a link immediately when loss/RTX or queueing delay rises).

If you want “match the throughput of the links”, adopt that explicitly: **weights ∝ estimated capacity**, *even when the source is under the sum capacity*. (It’s a product decision: slightly more jitter/out-of-order vs. resilience and fast failover.)

Below is the least-invasive way to get there with your current stack.

---

## The plan (minimal, surgical)

### A. Expose **receiver-side per-link delivery** in `ristsink` stats

Right now your sender stats per session include:

* `sent-original-packets`, `sent-retransmitted-packets`, `round-trip-time`.

Add fields from the **RTCP RR** for that session’s remote SSRC:

* `rr-packets-received`  (monotonic count from RR)
* `rr-fraction-lost`     (fraction lost in last interval)
* (optional) `rr-jitter`

You already find the `GstRTPSource` to read `rb-round-trip`. Read the **same** `stats` `GstStructure` for `rb-…` keys and pack them into your `rist/x-sender-session-stats`.

**C (in `gstristsink.c`, inside the per-session block that already fetches the `GstRTPSource *source` and `sstats`):**

```c
/* existing: rb-round-trip (Q16 NTP) pulled from sstats */
guint rb_rtt = 0;
gst_structure_get_uint (sstats, "rb-round-trip", &rb_rtt);
guint64 rtt_ns = gst_util_uint64_scale (rb_rtt, GST_SECOND, 65536);
gst_structure_set (session_stats, "round-trip-time", G_TYPE_UINT64, rtt_ns, NULL);

/* NEW: receiver report (RR) delivery signal */
guint rr_pkts = 0, rr_lost = 0, rr_exp = 0;
gdouble rr_frac = 0.0;
gst_structure_get_uint   (sstats, "rb-packets-received", &rr_pkts);
gst_structure_get_uint   (sstats, "rb-expected", &rr_exp);     /* if present */
gst_structure_get_uint   (sstats, "rb-packets-lost", &rr_lost);/* if present */
gst_structure_get_double (sstats, "rb-fraction-lost", &rr_frac);

gst_structure_set (session_stats,
   "rr-packets-received", G_TYPE_UINT, rr_pkts,
   "rr-fraction-lost",   G_TYPE_DOUBLE, rr_frac,
   NULL);
```

> If any of those keys aren’t present in your GStreamer build, keep the calls but handle “not found” (they’ll default to 0/0.0). The important one is **`rb/rr-packets-received`**.

### B. Consume those in the dispatcher; estimate capacity from **delivered** rate

Add fields to `LinkStats`:

```rust
prev_rr_received: u64,
ewma_delivered_pps: f64,  // from RR
```

Update them in `update_weights_from_stats(...)`:

```rust
let rr_recv = session_struct.get::<u64>("rr-packets-received").unwrap_or(0);
let delta_rr = rr_recv.saturating_sub(link.prev_rr_received) as f64;
let delivered_pps = delta_rr / delta_time;
link.ewma_delivered_pps = link.alpha * delivered_pps + (1.0 - link.alpha) * link.ewma_delivered_pps;
link.prev_rr_received = rr_recv;
```

Now change the EWMA scorer to use **delivered** instead of **sent-original**:

```rust
// Before: cap_est = stats.ewma_goodput / last_share;  // BAD: share-biased
// After:
let last_share = state.weights[i].max(share_floor);
let cap_meas   = stats.ewma_delivered_pps / last_share;  // receiver-based
let gp         = cap_meas.max(1.0).powf(0.5);            // spread compression
let q_rtx      = 1.0 / (1.0 + alpha * stats.ewma_rtx_rate);
let q_rtt      = 1.0 / (1.0 + beta  * (stats.ewma_rtt / 50.0).max(0.1));
let mut w      = (gp * q_rtx * q_rtt).max(1e-6);
```

> If `rr-packets-received` is missing (older `ristsink`), **fall back** to the old estimator, but gate it with stronger probing (next step) so you still get a signal.

### C. Keep your **post-normalization ε-mixing** and **max-link-share cap**

You’ve already implemented:

* ε-mix after normalize (great),
* waterfilling cap (e.g., `max-link-share = 0.65–0.70`).

Keep them. They’re exactly what production bonders do to avoid “all eggs in one basket” and oscillation.

### D. Add **deterministic micro‑probes** (when under total capacity)

Under light load there’s no congestion signal, so you still won’t *discover* capacity. Add a tiny rotating **boost** to one link each rebalance so you can learn without needing to overdrive the whole stream.

**State:**

```rust
probe_idx: usize,
probe_boost: f64,        // e.g., 0.12 (12% boost during the probe)
probe_period_ms: u64,    // e.g., 800 ms
last_probe: Instant,
```

**In `calculate_ewma_weights(...)`, right before normalization:**

```rust
// pick next link to probe every N ms
let now = Instant::now();
if now.duration_since(state.last_probe).as_millis() as u64 >= inner.probe_period_ms {
    state.probe_idx = (state.probe_idx + 1) % new_weights.len();
    state.last_probe = now;
}
// multiplicative bump on the current probe target
new_weights[state.probe_idx] *= 1.0 + inner.probe_boost;
```

Then **normalize → cap → ε-mix → commit** (reset SWRR debt when changed).

**How it helps:** if the probe link still shows low RTX and flat RTT slope during its bump, `ewma_delivered_pps / last_share` rises → your capacity estimate increases → the steady weights rise **even if** the total stream is below aggregate capacity. This is how devices like LiveU keep links “warmed” and proportional without constantly saturating the sum.

---

## Optional, but strongly recommended

### 1) Byte‑accurate scheduler (DRR), not packet‑count SWRR

RTP packets are *roughly* equal, but with H.265 they can vary. Switch your selection from “one buffer = one unit” to **deficit‑round‑robin (DRR)** using **bytes**:

* Maintain `deficit[i] += quantum * weight[i]` each cycle.
* Prefer the next pad whose `deficit >= buf.size()`, then decrement by `buf.size()`.

This makes “weights ∝ kb/s”, not packets/s, which is what you want when you say “match throughput”.

### 2) Quality guard rails (fast path)

If in a short window any link shows:

* `ewma_rtx_rate > 0.02–0.05`, or
* `rtt_slope > +X ms/s` (queueing inflating),
  then **halve** its instantaneous weight (or clamp at a scaled cap) for 1–2 intervals. You’re already computing those terms; just make them bite.

### 3) Priors (when you *know* the shaping in tests)

In your test harness you *know* `[1500,1250,750,300]`. Pass a **capacity prior** and blend:

```rust
dispatcher.set_property("capacity-prior", "[1500,1250,750,300]");
dispatcher.set_property("prior-weight", 0.4f64); // 40% prior + 60% measurement
```

Blend in the scorer:

```rust
let cap_hat = if prior > 0.0 { pw*prior + (1.0-pw)*cap_meas } else { cap_meas };
```

Priors make your test converge in seconds without needing to push the sum to saturation.

---

## How this addresses your goals

> “I want our distribution matching the throughput of the links.”

* With **receiver‑based delivered rate** and **share‑normalized cap estimation**, you’re finally measuring the right thing.
* With **micro‑probes**, you can learn capacity even when the stream is under the aggregate.
* With **ε‑mix + caps + fast guards**, you keep stability and avoid piling onto one path.

This is the same architecture families you’ll find in professional bonders:

* A **capacity estimator** driven by downstream feedback + micro‑probes.
* A **byte‑fair scheduler** that tracks those capacities.
* **Short‑loop quality brakes** so a bad path is clipped quickly.

---

## Concrete knobs to start with

* `probe_ratio (ε)`: 0.10 for first 5 s → 0.06 steady
* `max_link_share`: 0.65–0.70
* `rebalance_interval_ms`: 500
* `probe_boost`: 0.12; `probe_period_ms`: 800
* `ewma_rtx_penalty`: 0.30
* `ewma_rtt_penalty`: 0.10
* DRR `quantum`: one MTU (\~1200 bytes)

---

## If you can’t modify `ristsink` today

You can still get \~80% of the way there:

* Keep your **capacity‑normalized estimator** but replace the numerator with a **receiver‑side counter from `ristsrc`** (e.g., expose per‑session `received` over RTCP to the sender via a lightweight **APP** packet or a UDP control port). It’s one small bridge, and it avoids C changes in the sender.
* Or, **lean more on the probing loop**: the rotating +δ bump *will* inform capacity from RTX/RTT even without RR counts — just needs slightly stronger thresholds and smoothing.

---

### TL;DR

* **Change what you measure**: stop using “sent originals”; use **RR `packets-received`** per link.
* **Keep your good structural pieces** (ε‑mix, caps, SWRR reset).
* **Add micro‑probes** so you can learn under low load.
* **Prefer DRR (bytes)** for the actual scheduling.

If you want, I’ll inline patch your current `dispatcher.rs` to add `rr-packets-received` handling (with a graceful fallback), plus a compact DRR scheduler. And I can sketch the 10‑line C addition to `gstristsink.c` where you already read `rb-round-trip` so you can try this in one run.
