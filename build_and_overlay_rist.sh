#!/bin/bash
# Script to build GStreamer with RIST plugin from source and install system-wide

set -e

# Configuration
gstreamer_source_dir="/workspace/gstreamer"
build_dir="$gstreamer_source_dir/build"
install_prefix="/usr/local"

echo "============================================================"
echo "Building GStreamer from source with RIST and H.265 support"
echo "============================================================"

# Check if GStreamer source exists
if [ ! -d "$gstreamer_source_dir" ]; then
    echo "GStreamer source directory $gstreamer_source_dir does not exist."
    echo "Please clone GStreamer source first."
    exit 1
fi

cd "$gstreamer_source_dir"

# Clean previous build if requested
if [ "$1" = "--clean" ]; then
    echo "Cleaning previous build..."
    rm -rf "$build_dir"
fi

# Configure the build
if [ ! -d "$build_dir" ]; then
    echo "Setting up meson build with GPL support..."
    meson setup "$build_dir" \
        --prefix="$install_prefix" \
        -Dgpl=enabled \
        -Dbuildtype=release \
        -Ddoc=disabled \
        -Dexamples=disabled \
    -Dgst-examples=disabled \
        -Dtests=disabled \
        -Dintrospection=disabled \
        -Dpython=disabled \
        -Dlibav=enabled \
    -Ddevtools=disabled \
        -Drs=disabled \
        -Dsharp=disabled \
    -Dgtk=disabled \
    -Dgst-plugins-bad:codec2json=disabled \
    -Dgst-plugins-bad:fdkaac=disabled \
        -Dgst-plugins-bad:svtjpegxs=disabled \
        -Dgst-plugins-bad:webrtcdsp=disabled \
        -Dgst-plugins-bad:openh264=disabled \
    -Dgst-plugins-base:iso-codes=disabled
else
    echo "Build directory exists, reconfiguring..."
    cd "$build_dir"
                meson configure \
                -Dgpl=enabled \
                -Dlibav=enabled \
                -Ddevtools=disabled \
                    -Dgst-examples=disabled \
            -Dgst-plugins-bad:codec2json=disabled \
            -Dgst-plugins-bad:fdkaac=disabled \
            -Dgst-plugins-bad:svtjpegxs=disabled \
            -Dgst-plugins-bad:webrtcdsp=disabled \
            -Dgst-plugins-bad:openh264=disabled \
            -Dgst-plugins-base:iso-codes=disabled
fi

cd "$build_dir"

# Build GStreamer
echo "Building GStreamer (this may take a while)..."
ninja

# Install system-wide
echo "Installing GStreamer system-wide..."
ninja install

# Update library cache
echo "Updating library cache..."
ldconfig

echo "============================================================"
echo "GStreamer installation complete!"
echo "============================================================"

# Verify installation
echo "Verifying GStreamer installation..."
export PKG_CONFIG_PATH="$install_prefix/lib/pkgconfig:$install_prefix/lib/x86_64-linux-gnu/pkgconfig:$PKG_CONFIG_PATH"
export LD_LIBRARY_PATH="$install_prefix/lib:$install_prefix/lib/x86_64-linux-gnu:$LD_LIBRARY_PATH"
export PATH="$install_prefix/bin:$PATH"
export GST_PLUGIN_PATH="$install_prefix/lib/gstreamer-1.0:$install_prefix/lib/x86_64-linux-gnu/gstreamer-1.0"

# Check GStreamer version
echo "GStreamer version:"
gst-inspect-1.0 --version

# Check for H.265 encoder
echo ""
echo "Checking H.265 encoder availability:"
if gst-inspect-1.0 x265enc > /dev/null 2>&1; then
    echo "✓ H.265 encoder (x265enc) is available"
else
    echo "✗ H.265 encoder (x265enc) not found"
fi

# Check for RIST plugin elements
echo ""
echo "Checking RIST plugin elements:"
rist_elements=("ristsrc" "ristsink" "ristrtxsend" "ristrtxreceive")
for element in "${rist_elements[@]}"; do
    if gst-inspect-1.0 "$element" > /dev/null 2>&1; then
        echo "✓ $element is available"
    else
        echo "✗ $element not found"
    fi
done

echo ""
echo "============================================================"
echo "Installation Summary:"
echo "- GStreamer installed to: $install_prefix"
echo "- Plugin directory: $GST_PLUGIN_PATH"
echo "- Library directory: $LD_LIBRARY_PATH"
echo ""
echo "To use this installation, ensure these environment variables are set:"
echo "export PKG_CONFIG_PATH=\"$install_prefix/lib/pkgconfig:$install_prefix/lib/x86_64-linux-gnu/pkgconfig:\$PKG_CONFIG_PATH\""
echo "export LD_LIBRARY_PATH=\"$install_prefix/lib:$install_prefix/lib/x86_64-linux-gnu:\$LD_LIBRARY_PATH\""
echo "export PATH=\"$install_prefix/bin:\$PATH\""
echo "export GST_PLUGIN_PATH=\"$install_prefix/lib/gstreamer-1.0:$install_prefix/lib/x86_64-linux-gnu/gstreamer-1.0\""
echo "============================================================"
