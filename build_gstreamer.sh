#!/usr/bin/env bash
# Build the vendored GStreamer tree and stage the patched RIST plugin locally.

set -euo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
GSTREAMER_DIR="$ROOT_DIR/gstreamer"
BUILD_DIR="${BUILD_DIR:-$GSTREAMER_DIR/builddir}"
TARGET_ROOT="${TARGET_ROOT:-$ROOT_DIR/target/gstreamer}"
PREFIX="${INSTALL_PREFIX:-$TARGET_ROOT/install}"
OVERLAY_DIR="${OVERLAY_DIR:-$TARGET_ROOT/overlay}"
ENV_FILE="$TARGET_ROOT/env.sh"

usage() {
    cat <<USAGE
Usage: ./build_gstreamer.sh [--clean]

Builds the vendored GStreamer sources with the patched RIST plugin and stages
an overlay under $TARGET_ROOT so it can be used without touching /usr/local.

Options:
  --clean   Remove the build directory and staged artifacts before building.
  -h, --help Print this help message and exit.
USAGE
}

if [[ ! -d "$GSTREAMER_DIR" ]]; then
    echo "error: expected gstreamer submodule at $GSTREAMER_DIR" >&2
    echo "run 'git submodule update --init --recursive gstreamer' and retry" >&2
    exit 1
fi

CLEAN=false
while [[ $# -gt 0 ]]; do
    case "$1" in
        --clean)
            CLEAN=true
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "error: unknown option '$1'" >&2
            usage >&2
            exit 1
            ;;
    esac
    shift
end

if $CLEAN; then
    echo "==> Cleaning previous artifacts"
    rm -rf "$BUILD_DIR" "$TARGET_ROOT"
fi

mkdir -p "$TARGET_ROOT"

MESON_FLAGS=(
    "--prefix=$PREFIX"
    "--libdir=lib"
    "--buildtype=release"
    "--default-library=shared"
    "--wrap-mode=nofallback"
    "-Dgpl=enabled"
    "-Dlibav=enabled"
    "-Ddoc=disabled"
    "-Dexamples=disabled"
    "-Dtests=disabled"
    "-Dintrospection=disabled"
    "-Dpython=disabled"
    "-Dgst-plugins-bad:rist=enabled"
)

pushd "$GSTREAMER_DIR" >/dev/null
if [[ ! -d "$BUILD_DIR" ]]; then
    echo "==> Configuring GStreamer (meson setup)"
    meson setup "$BUILD_DIR" "${MESON_FLAGS[@]}"
else
    echo "==> Refreshing build configuration"
    meson setup "$BUILD_DIR" --reconfigure "${MESON_FLAGS[@]}"
fi

echo "==> Compiling patched GStreamer components"
meson compile -C "$BUILD_DIR"

echo "==> Installing into $PREFIX"
meson install -C "$BUILD_DIR"
popd >/dev/null

mkdir -p "$OVERLAY_DIR"

copy_plugins() {
    local src="$1"
    if [[ -d "$src/gstreamer-1.0" ]]; then
        rsync -a --delete "$src/gstreamer-1.0/" "$OVERLAY_DIR/"
    fi
}

# Stage plugin directories from common lib paths.
copy_plugins "$PREFIX/lib"
copy_plugins "$PREFIX/lib64"
copy_plugins "$PREFIX/lib/x86_64-linux-gnu"

if [[ ! -d "$OVERLAY_DIR" || -z $(ls -A "$OVERLAY_DIR" 2>/dev/null) ]]; then
    echo "warning: no plugins were staged under $OVERLAY_DIR" >&2
    echo "check the build output above for install errors" >&2
fi

mkdir -p "$TARGET_ROOT"
cat > "$ENV_FILE" <<ENV
# shellcheck disable=SC1090
# Source this file with: source "$ENV_FILE"
export GSTREAMER_PREFIX="$PREFIX"
export PATH="$PREFIX/bin:\\${PATH:-}"
export LD_LIBRARY_PATH="$PREFIX/lib:$PREFIX/lib64:$PREFIX/lib/x86_64-linux-gnu:\\${LD_LIBRARY_PATH:-}"
export PKG_CONFIG_PATH="$PREFIX/lib/pkgconfig:$PREFIX/lib64/pkgconfig:$PREFIX/lib/x86_64-linux-gnu/pkgconfig:\\${PKG_CONFIG_PATH:-}"
export GST_PLUGIN_PATH="$OVERLAY_DIR:$PREFIX/lib/gstreamer-1.0:$PREFIX/lib64/gstreamer-1.0:$PREFIX/lib/x86_64-linux-gnu/gstreamer-1.0:\\${GST_PLUGIN_PATH:-}"
ENV

cat <<SUMMARY

Patch build complete.

• Prefix:          $PREFIX
• Overlay plugins: $OVERLAY_DIR
• Env helper:      source $ENV_FILE

After sourcing the env helper, verify the patched plugin with:
  gst-inspect-1.0 ristsink | grep -A2 "rist/x-sender-session-stats"
SUMMARY
