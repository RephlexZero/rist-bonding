# Dev container for RIST bonding development (build GStreamer from source)
FROM ubuntu:latest

# Avoid interactive prompts
ENV DEBIAN_FRONTEND=noninteractive

# --------------------------------------------------------------------
# Install build dependencies for GStreamer and RIST
# --------------------------------------------------------------------
RUN apt-get update && apt-get install -y --no-install-recommends \
    # Core build tools
    build-essential \
    git \
    curl \
    pkg-config \
    ninja-build \
    cmake \
    flex \
    bison \
    nasm \
    yasm \
    python3 \
    python3-pip \
    # GStreamer build dependencies (libraries only, no runtime)
    libglib2.0-dev \
    libunwind-dev \
    libdw-dev \
    liborc-0.4-dev \
    libssl-dev \
    libfontconfig1-dev \
    libfreetype6-dev \
    libjpeg-dev \
    libpng-dev \
    libxml2-dev \
    libsoup-3.0-dev \
    libx11-dev \
    libxv-dev \
    libgl1-mesa-dev \
    libegl1-mesa-dev \
    libgles2-mesa-dev \
    libdrm-dev \
    libxrandr-dev \
    libgraphene-1.0-dev \
    # Video/Audio codec dependencies
    libx264-dev \
    libx265-dev \
    x265 \
    libvpx-dev \
    libopus-dev \
    libvorbis-dev \
    libogg-dev \
    libtheora-dev \
    libflac-dev \
    libmp3lame-dev \
    libmpg123-dev \
    # Additional dependencies for plugins
    libcairo2-dev \
    libpango1.0-dev \
    libjpeg-dev \
    libpng-dev \
    libwebp-dev \
    libnice-dev \
    libsrtp2-dev \
    # Networking/debug utilities
    iproute2 \
    net-tools \
    iputils-ping \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Install newer meson version via pip
RUN pip3 install --break-system-packages meson

# --------------------------------------------------------------------
# Install Rust
# --------------------------------------------------------------------
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# --------------------------------------------------------------------
# Workspace
# --------------------------------------------------------------------
WORKDIR /workspace

# Default command
CMD ["bash"]
