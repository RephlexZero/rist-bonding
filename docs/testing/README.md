# Testing

The project uses Cargo's test framework. Some tests require GStreamer
components that are not available in a minimal environment.

## Prerequisites

- GStreamer with the `ristsrc` and `ristsink` elements installed.
- The test harness plugins (enabled via the default `test-plugin` feature)
  that provide the `counter_sink` element.

## Running

Simply execute:

```bash
cargo test
```

The `network_integration` test provisions two simulated links via the
`NetworkOrchestrator` and verifies end‑to‑end RIST bonding.
