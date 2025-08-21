#!/usr/bin/env bash
set -euo pipefail

# Grant CAP_SYS_ADMIN, CAP_NET_ADMIN, and CAP_NET_RAW to the compiled test binaries
# so they can create/manage network namespaces and veth pairs without sudo.
#
# Usage:
#   ./scripts/grant_caps.sh
# Then run the built test binaries directly (no sudo), e.g.:
#   target/debug/deps/integration_tests-<hash> --nocapture
#   target/debug/deps/automated_integration-<hash> --nocapture
#
# Notes:
# - You must re-run this script after re-compiling if the filenames change.
# - This requires 'sudo' one time to set file capabilities.
# - Ensure 'setcap' (from libcap) is installed: sudo apt install -y libcap2-bin

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

echo "[grant_caps] Building tests (no-run) for integration_tests..."
cargo test -p integration_tests --no-run

echo "[grant_caps] Locating test binaries..."
mapfile -t BINARIES < <(ls -1 target/debug/deps | grep -E '^(integration_tests|automated_integration)-[0-9a-f]+$' || true)

if [[ ${#BINARIES[@]} -eq 0 ]]; then
  echo "[grant_caps] No test binaries found under target/debug/deps."
  echo "             Run: cargo test -p integration_tests --no-run"
  exit 1
fi

echo "[grant_caps] Granting capabilities (requires sudo):"
for bin in "${BINARIES[@]}"; do
  path="target/debug/deps/$bin"
  echo "  -> $path"
  sudo setcap cap_sys_admin,cap_net_admin,cap_net_raw+ep "$path"
  (command -v getcap >/dev/null 2>&1 && getcap "$path") || true
done

echo "[grant_caps] Done. You can now run the test binaries without sudo."
echo "             Remember to re-run this script after rebuilding if filenames change."
