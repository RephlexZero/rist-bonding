#!/usr/bin/env bash
set -euo pipefail

# Best practice wrapper to run netns-based automated integration tests with sudo
# without rebuilding as root. Builds as the invoking user first, then runs tests
# with sudo preserving necessary environment.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# Ensure artifacts directory is predictable
export TEST_ARTIFACTS_DIR="${ROOT_DIR}/target/test-artifacts"
mkdir -p "$TEST_ARTIFACTS_DIR"

# Optional: preserve target dir to avoid rebuild as root
export CARGO_TARGET_DIR="${ROOT_DIR}/target"

# Build the test binary as the current user
cargo test -p rist-elements --test integration_tests --no-run

# Variables to preserve for GStreamer plugins and runtime
ENV_KEEP=(
  "CARGO_TARGET_DIR=${CARGO_TARGET_DIR}"
  "TEST_ARTIFACTS_DIR=${TEST_ARTIFACTS_DIR}"
  "RUST_LOG=${RUST_LOG:-}"
  "GST_DEBUG=${GST_DEBUG:-}"
  "GST_PLUGIN_PATH=${GST_PLUGIN_PATH:-}"
  "RIST_SAVE_MP4=1"
)

echo "Running automated integration tests with sudo (netns required)"
sudo -E env "${ENV_KEEP[@]}" cargo test -p rist-elements --test integration_tests -- --nocapture
