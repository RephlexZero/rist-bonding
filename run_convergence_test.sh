#!/usr/bin/env bash
# Run the canonical convergence test with reasonable defaults.

set -euo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ENV_FILE="$ROOT_DIR/target/gstreamer/env.sh"

if [[ -f "$ENV_FILE" ]]; then
    # shellcheck disable=SC1090
    source "$ENV_FILE"
else
    echo "warning: $ENV_FILE not found; run ./build_gstreamer.sh first to stage the patched plugin" >&2
fi

export GST_DEBUG="${GST_DEBUG:-ristdispatcher:INFO,ristrtxsend:ERROR,*:WARNING}"
export RUST_LOG="${RUST_LOG:-rist_elements=info}"

cargo test -p rist-elements bonded_links_static_stress -- --nocapture "$@" \
  2>&1 | sed '/requested seqnum/d'
