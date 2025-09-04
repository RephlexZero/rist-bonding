#!/bin/bash
# Script to set up environment variables for custom-built GStreamer

export GSTREAMER_PREFIX="/usr/local"

# Add GStreamer binaries to PATH
export PATH="$GSTREAMER_PREFIX/bin:$PATH"

# Add GStreamer libraries to library path
export LD_LIBRARY_PATH="$GSTREAMER_PREFIX/lib:$GSTREAMER_PREFIX/lib/x86_64-linux-gnu:$LD_LIBRARY_PATH"

# Add GStreamer pkg-config files
export PKG_CONFIG_PATH="$GSTREAMER_PREFIX/lib/pkgconfig:$GSTREAMER_PREFIX/lib/x86_64-linux-gnu/pkgconfig:$PKG_CONFIG_PATH"

# Add GStreamer plugins path
export GST_PLUGIN_PATH="$GSTREAMER_PREFIX/lib/gstreamer-1.0:$GSTREAMER_PREFIX/lib/x86_64-linux-gnu/gstreamer-1.0"

# Optional: Set GStreamer debug level (uncomment if needed)
# export GST_DEBUG=3

echo "GStreamer environment configured:"
echo "- PREFIX: $GSTREAMER_PREFIX"
echo "- PATH: $PATH"
echo "- LD_LIBRARY_PATH: $LD_LIBRARY_PATH"
echo "- PKG_CONFIG_PATH: $PKG_CONFIG_PATH"
echo "- GST_PLUGIN_PATH: $GST_PLUGIN_PATH"

# If sourced, export the variables for the current shell
if [[ "${BASH_SOURCE[0]}" != "${0}" ]]; then
    echo ""
    echo "Environment variables exported to current shell."
    echo "You can now use gst-launch-1.0, gst-inspect-1.0, etc."
fi
