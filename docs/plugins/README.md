# Plugins

This workspace ships experimental GStreamer elements focused on RIST bonding
and adaptive control. They are exposed through the `rist-elements` crate.

## Provided elements

- `ristsrc` – receives RIST streams and exposes them as standard GStreamer
  buffers.
- `ristsink` – sends buffers over RIST with optional link bonding.
- `ristdispatcher` – distributes buffers across multiple RIST sessions using
  smooth weighted round robin and optional automatic rebalancing.
- `dynbitrate` – monitors link statistics and adjusts an upstream encoder's
  bitrate while coordinating with `ristdispatcher` for unified control.

### `ristdispatcher`

`ristdispatcher` sits between an upstream encoder and a `ristsink` and fan-outs
buffers to one or more RIST sessions. Each output is treated as a separate
link and assigned a weight. The element uses a Smooth Weighted Round Robin
algorithm to pick the next link for every buffer. When the
`auto-balance` feature is enabled (the default), it periodically polls the
`ristsink` for per-session statistics such as retransmission counts and round
trip time. These metrics feed an exponential weighted moving average that in
turn updates the link weights, pushing more traffic onto healthier paths while
respecting a hysteresis window to avoid flapping. It can also optionally
duplicate keyframes across links to speed up failover.

### `dynbitrate`

`dynbitrate` is a control-only element placed after the encoder. It reads
statistics from a `ristsink` and drives the encoder's bitrate property based on
packet loss and RTT targets. The controller uses a configurable step size and
rate limiting to gently increase or decrease the bitrate. When a dispatcher is
provided via the `dispatcher` property, `dynbitrate` also derives per-link
weights from the same stats and sets them on the dispatcher so that bitrate and
link selection react in concert.

Additional testing elements such as `counter_sink` are available through the
`test-plugin` feature and are intended only for use in the test suite.
