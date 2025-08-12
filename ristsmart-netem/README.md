# Ristsmart-netem: Network Emulation for RIST Testing

A Rust library for network emulation specifically designed for testing RIST (Reliable Internet Stream Transport) bonding and adaptive algorithms. This implementation provides per-link network namespaces with Ornstein-Uhlenbeck throughput variation and Gilbert-Elliott burst loss modeling.

## Quick Start

### Library Usage

```rust
use ristsmart_netem::{EmulatorBuilder, LinkSpec, OUParams, GEParams, DelayProfile, RateLimiter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create a cellular link with variable throughput and burst loss
    let cellular_link = LinkSpec {
        name: "cellular".to_string(),
        rate_limiter: RateLimiter::Tbf,
        ou: OUParams {
            mean_bps: 2_000_000,    // 2 Mbps average
            tau_ms: 2000,           // 2 second mean reversion
            sigma: 0.25,            // 25% volatility
            tick_ms: 200,           // Update every 200ms
        },
        ge: GEParams {
            p_good: 0.001,          // 0.1% loss in good state
            p_bad: 0.08,            // 8% loss in bad state
            p: 0.01,                // 1% chance good->bad
            r: 0.15,                // 15% chance bad->good
        },
        delay: DelayProfile {
            delay_ms: 60,
            jitter_ms: 15,
            reorder_pct: 0.1,
        },
        ifb_ingress: false,
    };

    // Build and start emulator
    let mut builder = EmulatorBuilder::new();
    builder.add_link(cellular_link).with_seed(42);
    
    let emulator = builder.build().await?;
    emulator.start().await?;

    // Your RIST sender can now send to 10.0.0.2:5000
    // Set up packet forwarding to your receiver
    if let Some(link) = emulator.link("cellular") {
        link.bind_forwarder(5000, "127.0.0.1", 6000).await?;
    }

    // Run your RIST test...
    tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;

    // Collect metrics
    let metrics = emulator.metrics().await?;
    println!("Collected metrics from {} links", metrics.links.len());

    // Clean shutdown
    emulator.teardown().await?;
    Ok(())
}
```

### JSON Configuration

Create `scenario.json`:
```json
{
  "links": [
    {
      "name": "cellular",
      "rate_limiter": "Tbf",
      "ou": {
        "mean_bps": 2000000,
        "tau_ms": 2000,
        "sigma": 0.25,
        "tick_ms": 200
      },
      "ge": {
        "p_good": 0.001,
        "p_bad": 0.08,
        "p": 0.01,
        "r": 0.15
      },
      "delay": {
        "delay_ms": 60,
        "jitter_ms": 15,
        "reorder_pct": 0.1
      },
      "ifb_ingress": false
    }
  ],
  "seed": 42
}
```

Load and run:
```rust
let builder = EmulatorBuilder::from_file("scenario.json").await?;
let emulator = builder.build().await?;
```

### Command Line Interface

```bash
# Run a scenario for 60 seconds with metrics output
cargo run --bin emulator -- run \
    --scenario examples/cellular.json \
    --duration 60 \
    --metrics /tmp/metrics.jsonl

# Start emulator in background
cargo run --bin emulator -- up --scenario examples/dual_cellular.json

# Stop emulator
cargo run --bin emulator -- down --scenario examples/dual_cellular.json
```

## Testing RIST Pipelines

### Basic Setup

1. **Start the emulator**:
```bash
cargo run --bin emulator -- run --scenario examples/cellular.json --duration 300
```

2. **Configure your RIST sender** to send to the namespace IPs:
   - Link 0: `10.0.0.2:5000` 
   - Link 1: `10.1.0.2:5001`
   - etc.

3. **Configure your RIST receiver** to listen on forwarded ports:
   - Receiver on `127.0.0.1:6000`, `127.0.0.1:6001`, etc.

4. **Set up forwarders** (programmatically):
```rust
emulator.link("cellular").unwrap()
    .bind_forwarder(5000, "127.0.0.1", 6000).await?;
```

### GStreamer Example

**Sender** (send to emulated links):
```bash
gst-launch-1.0 videotestsrc ! x264enc ! \
    ristsink address=10.0.0.2 port=5000
```

**Receiver** (receive from forwarded port):
```bash
gst-launch-1.0 ristsrc address=127.0.0.1 port=6000 ! \
    decodebin ! autovideosink
```

## Key Features

- **Per-link network namespaces**: Isolated network environments
- **OU throughput variation**: Realistic cellular-like bandwidth changes  
- **GE burst loss**: Correlated packet loss in good/bad states
- **Netem effects**: Configurable delay, jitter, reorder
- **Dynamic updates**: Change parameters during runtime
- **Metrics collection**: Real-time link statistics
- **JSON configuration**: Reproducible test scenarios

## Requirements

- **Linux**: Network namespaces and traffic control
- **Privileges**: `CAP_NET_ADMIN` or root for network operations
- **Dependencies**: `iproute2` tools (`ip`, `tc`) must be available

## Running Tests  

**Unit tests** (no privileges required):
```bash
cargo test
```

**Integration tests** (requires privileges):
```bash
RISTS_PRIV=1 sudo -E cargo test -- --ignored --test-threads=1
```

## Architecture

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Main NS       │    │   Link NS 0     │    │   Link NS 1     │ 
│                 │    │                 │    │                 │
│  RIST Sender ───┼────┤→ 10.0.0.2:5000  │    │→ 10.1.0.2:5001  │
│  RIST Receiver  │    │    │ OU+GE+Netem│    │    │ OU+GE+Netem│
│                 │←───┤← Forwarder      │    │← Forwarder      │
└─────────────────┘    └─────────────────┘    └─────────────────┘
         ↑                       ↑                       ↑
         │                   Veth Pair              Veth Pair
         └───────────────────────┴───────────────────────┘
```

Each link gets its own network namespace with:
- **TBF/CAKE**: Rate limiting with OU-driven changes
- **Netem**: Delay, jitter, reorder, GE-driven loss
- **Forwarder**: Optional UDP proxy back to main namespace

This creates realistic network conditions for testing RIST bonding, failover timing, and adaptive bitrate algorithms.
