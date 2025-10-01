#!/usr/bin/env bash
set -euo pipefail

export GST_DEBUG="${GST_DEBUG:-ristdispatcher:INFO,ristrtxsend:ERROR,*:WARNING}"

cargo test -p rist-elements --test bonded_links_static_stress -- --nocapture \
  2>&1 | sed '/requested seqnum/d'
