#!/bin/bash

# RIST Sender with Bonding
set -e

echo "Starting RIST sender with bonding..."

export GST_PLUGIN_PATH=/usr/lib/x86_64-linux-gnu/gstreamer-1.0

# Parse bonding addresses from environment
BONDING_ADDRESSES=${BONDING_ADDRESSES:-"localhost:5004"}
VIDEO_BITRATE=${VIDEO_BITRATE:-4000}
VIDEO_WIDTH=${VIDEO_WIDTH:-1920}
VIDEO_HEIGHT=${VIDEO_HEIGHT:-1080}
VIDEO_FPS=${VIDEO_FPS:-30}

echo "Configuration:"
echo "  Bonding addresses: $BONDING_ADDRESSES"
echo "  Video bitrate: ${VIDEO_BITRATE}kbps"
echo "  Resolution: ${VIDEO_WIDTH}x${VIDEO_HEIGHT}@${VIDEO_FPS}fps"

# Create a simple test pipeline with the bonding plugin
exec gst-launch-1.0 -v \
    videotestsrc pattern=ball \
    ! video/x-raw,width=$VIDEO_WIDTH,height=$VIDEO_HEIGHT,framerate=$VIDEO_FPS/1 \
    ! x264enc bitrate=$VIDEO_BITRATE tune=zerolatency speed-preset=ultrafast \
    ! rtph264pay pt=96 \
    ! ristdispatcher name=disp auto-balance=true strategy=ewma \
    ! ristsink bonding-addresses="$BONDING_ADDRESSES"