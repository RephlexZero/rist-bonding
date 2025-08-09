#!/bin/bash

# RIST Receiver with Statistics Collection
set -e

echo "Starting RIST receiver with statistics collection..."

export GST_PLUGIN_PATH=/usr/lib/x86_64-linux-gnu/gstreamer-1.0

# Start statistics collection server
python3 scripts/stats-server.py &
STATS_PID=$!

# Cleanup function
cleanup() {
    echo "Shutting down..."
    kill $STATS_PID 2>/dev/null || true
    exit 0
}
trap cleanup SIGTERM SIGINT

# Start the receiver pipeline
exec gst-launch-1.0 -v \
    ristsrc \
    ! rtph264depay \
    ! avdec_h264 \
    ! videoconvert \
    ! autovideosink