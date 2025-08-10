# Implementation Plan: Multi-Link Network Emulation in Rust for RIST Bonding Testing

## Overview

This plan outlines the full implementation of a **realistic multi-link network emulator** for stress testing a RIST bonding solution using Rust. The goal is to simulate **cellular-like network conditions** including:

* Fluctuating throughput via **Ornstein–Uhlenbeck (OU) process**
* **Gilbert–Elliott (GE) burst loss model**
* Variable delay and jitter
* Multi-namespace isolation for independent link simulation
* Integration with GStreamer-based RIST sender/receiver

The design prioritizes precision, reproducibility, and in-process control without shelling out to external `tc` commands.

---

## Architecture

### 1. **Network Namespace Per Link**

* Each simulated link runs inside its own **Linux network namespace**.
* A `veth` pair connects each namespace to the main test namespace.
* This isolation allows per-link control of bandwidth, delay, and loss.

### 2. **Traffic Control Components**

* **TBF or CAKE** for precise bandwidth control.
* **netem** for delay, jitter, reordering, and packet loss.
* **Gilbert–Elliott** model inside netem to simulate bursty loss.
* **IFB devices** for ingress shaping.

### 3. **Control Loop**

* OU process updates the rate in TBF/CAKE every N milliseconds.
* GE parameters remain constant or can be adjusted dynamically for more realism.
* Direct netlink calls update qdisc parameters without spawning external processes.

---

## Rust Implementation

### 1. **Core Crates**

* [`neli`](https://crates.io/crates/neli) – low-level netlink bindings.
* [`rtnetlink`](https://crates.io/crates/rtnetlink) – high-level link and address management.
* [`netlink-packet-qdisc`](https://crates.io/crates/netlink-packet-qdisc) – qdisc configuration.
* [`nix`](https://crates.io/crates/nix) – namespace handling (`setns`).

### 2. **Namespace Setup**

```rust
use nix::sched::{setns, CloneFlags};
use std::fs::File;

fn enter_namespace(path: &str) -> nix::Result<()> {
    let fd = File::open(path)?;
    setns(fd.as_raw_fd(), CloneFlags::CLONE_NEWNET)
}
```

* Create namespaces and veth pairs during init phase.
* Assign IPs and bring interfaces up.

### 3. **TBF/CAKE Rate Limiting**

* Apply TBF or CAKE qdisc as `root` qdisc in each namespace.
* OU process samples the next rate and updates qdisc with `change`.

### 4. **Netem + Gilbert–Elliott Loss**

* Attach netem qdisc under TBF/CAKE using `parent` handle.
* Configure `loss gemodel` with:

  * `p_good`: loss in good state.
  * `p_bad`: loss in bad state.
  * `p`: prob. of good→bad.
  * `r`: prob. of bad→good.

### 5. **Ingress Shaping**

* Use IFB devices and ingress filters to apply shaping to incoming traffic.

### 6. **Dynamic Control Loop**

```rust
loop {
    let new_rate = ou_process_sample();
    update_tbf_rate(interface, new_rate).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
}
```

* Each namespace has its own OU-driven rate schedule.
* GE loss runs continuously in netem.

---

## Testing Strategy

1. **Baseline test** – Single link, static rate, no loss.
2. **Rate variation only** – OU-driven TBF, no loss.
3. **Loss only** – Static rate, Gilbert–Elliott loss.
4. **Combined** – OU + GE for each link.
5. **Multi-link bonding** – All links active, RIST bonding enabled.

Metrics:

* Effective throughput per link.
* RIST packet retransmission rate.
* Bonding failover and recovery time.

---

## Future Enhancements

* Integrate **real-world cellular traces** via Mahimahi for even more realism.
* Add netem `slot` option for simulating LTE/5G scheduling bursts.
* Introduce congestion models for testing QoS responsiveness.

---

## Deliverables

* Rust binary that:

  * Creates namespaces and veth pairs.
  * Configures TBF/CAKE + netem + Gilbert–Elliott.
  * Runs OU process to vary bandwidth.
  * Optionally logs per-link stats.
* Documentation on configuration parameters.
* Example GStreamer pipelines for RIST bonding tests.
