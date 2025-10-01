# Devcontainer

VS Code configuration for a ready-to-test bonding workspace.

## What You Get

- Rust toolchain + rust-analyzer
- Meson/Ninja + build deps for the patched GStreamer tree
- Preinstalled GStreamer runtime with NET_ADMIN/SYS_ADMIN capabilities
- Helper scripts (`build_gstreamer.sh`, `run_test.sh`) on PATH

## Usage

1. Install VS Code + Dev Containers extension.
2. `code .` and choose **Reopen in Container**.
3. The container bootstraps the patched GStreamer build; re-run `./build_gstreamer.sh` when upstream submodule updates.

Inside the container:

```bash
cargo build --all-features
cargo test -p rist-elements bonded_links_static_stress -- --nocapture
```

`GST_PLUGIN_PATH` and relevant capabilities are already configured so the dispatcher can consume the custom RIST telemetry without extra setup.
