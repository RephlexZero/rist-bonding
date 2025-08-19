# RIST Bonding

This workspace contains experimental GStreamer elements for RIST bonding and
dynamic bitrate control. It also ships a small network simulator used in tests.

## Network integration test

An end-to-end integration test lives in
`ristsmart/tests/network_integration.rs`. The test spins up a simulated network
with two links, sends audio through `ristdispatcher` and `ristsink`, and
receives the stream with `ristsrc` and a `counter_sink`.

The test is ignored by default because it requires additional GStreamer
components to be installed.

### Prerequisites

- GStreamer with the `ristsrc` and `ristsink` transport elements available.
- Test harness plugins (enabled via the default `test-plugin` feature) providing
  the `counter_sink` element.

### Running the test

Enable ignored tests when executing `cargo test`:

```bash
cargo test --test network_integration -- --ignored
```

This will run the integration test alongside the other tests in the file.
