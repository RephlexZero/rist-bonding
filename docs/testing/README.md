# Testing

The project uses Cargo's test framework. Some tests require GStreamer
components that are not available in a minimal environment.

## Prerequisites

- GStreamer with the `ristsrc` and `ristsink` elements installed.
- The test harness plugins (enabled via the default `test-plugin` feature)
  that provide the `counter_sink` element.

## Running

Most tests can be run normally:

```bash
cargo test
```

Some integration tests use Linux network namespaces (netns) via the
`netns-testbench` orchestrator. These require CAP_SYS_ADMIN (and typically
CAP_NET_ADMIN/CAP_NET_RAW) to create namespaces and veth pairs.

You have three options:

1) Run those tests with sudo (quickest):

```bash
sudo -E cargo test -p integration_tests -- --nocapture
```

2) Grant capabilities to built test binaries (no sudo at runtime):

```bash
# One-time per build (filenames include a hash; re-run after rebuilds)
./scripts/grant_caps.sh

# Then run the binaries directly without sudo:
target/debug/deps/integration_tests-<hash> --nocapture
target/debug/deps/automated_integration-<hash> --nocapture
```

This script uses `setcap` to grant `cap_sys_admin,cap_net_admin,cap_net_raw+ep`.
Install it with `sudo apt install -y libcap2-bin` if missing.

3) Configure sudoers NOPASSWD (CI-friendly):

Add a rule for your user to run `ip`, `ip netns`, and the test binaries without a password.
For example (adjust paths/users carefully):

```
%yourgroup ALL=(root) NOPASSWD: /usr/sbin/ip, /usr/sbin/ip netns *, /home/you/Documents/rust/rist-bonding/target/debug/deps/*
```

### Useful env vars

- `RIST_SHOW_VIDEO=1` to display a preview of the received H.265 video during tests.
- `RIST_REQUIRE_BUFFERS=1` to fail the automated integration test if no buffers are observed.
- `GST_DEBUG=rist*:5` for detailed RIST element logging.

### Troubleshooting

- "Permission denied" or namespace cleanup warnings: ensure you ran with sudo, granted capabilities, or used NOPASSWD sudoers.
- No video window: preview is off by default; set `RIST_SHOW_VIDEO=1`. In headless environments, rely on buffer logs instead.
- No buffers observed: try increasing flow duration or enable `GST_DEBUG=rist*:5`.
