#!/bin/bash
# Development helper script for VS Code devcontainer
# This script provides common development tasks in the containerized environment

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" &> /dev/null && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

print_help() {
    cat << EOF
RIST Bonding Development Helper

Usage: $0 <command> [options]

Commands:
    setup           - Initial setup and dependency check
    build           - Build all crates
    test            - Run all tests
    test-unit       - Run only unit tests
    test-integration - Run only integration tests
    test-network    - Run network simulation tests
    lint            - Run clippy with strict settings
    format          - Format all code
    clean           - Clean build artifacts
    docs            - Generate and serve documentation
    benchmark       - Run performance benchmarks
    gst-inspect     - Inspect GStreamer plugins
    network-check   - Verify network capabilities
    help            - Show this help message

Examples:
    $0 setup          # Initial setup
    $0 build          # Build everything
    $0 test           # Run all tests
    $0 test-network   # Test network simulation
    $0 docs           # Generate documentation
    $0 gst-inspect    # List GStreamer plugins

Environment Variables:
    RUST_LOG=debug    # Enable debug logging
    RUST_BACKTRACE=1  # Show backtraces on panic
EOF
}

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

check_dependencies() {
    log_info "Checking development dependencies..."
    
    # Check Rust
    if ! command -v cargo &> /dev/null; then
        log_error "Cargo not found. Please install Rust."
        return 1
    fi
    
    # Check GStreamer
    if ! command -v gst-inspect-1.0 &> /dev/null; then
        log_error "GStreamer not found."
        return 1
    fi
    
    # Check network tools
    if ! command -v ip &> /dev/null; then
        log_error "iproute2 not found."
        return 1
    fi
    
    log_success "All dependencies available"
    return 0
}

setup_environment() {
    log_info "Setting up development environment..."
    
    cd "$PROJECT_ROOT"
    
    # Check dependencies
    check_dependencies || return 1
    
    # Update Rust toolchain
    log_info "Updating Rust toolchain..."
    rustup update stable
    rustup component add clippy rustfmt
    
    # Install cargo tools if not present
    if ! command -v cargo-watch &> /dev/null; then
        log_info "Installing cargo-watch..."
        cargo install cargo-watch
    fi
    
    # Verify GStreamer RIST plugin
    log_info "Checking GStreamer RIST plugin..."
    if gst-inspect-1.0 ristsrc &> /dev/null; then
        log_success "GStreamer RIST plugin available"
    else
        log_warning "GStreamer RIST plugin not found - some tests may fail"
    fi
    
    # Check network capabilities
    log_info "Verifying network capabilities..."
    if ip netns list &> /dev/null; then
        log_success "Network namespace support available"
    else
        log_warning "Network namespace support not available"
    fi
    
    log_success "Development environment setup complete"
}

build_project() {
    log_info "Building project..."
    cd "$PROJECT_ROOT"
    
    # Build with all features
    cargo build --all-features --all-targets
    
    log_success "Build completed"
}

run_tests() {
    log_info "Running all tests..."
    cd "$PROJECT_ROOT"
    
    # Run the comprehensive test suite
    if [[ -x "./scripts/docker-test.sh" ]]; then
        ./scripts/docker-test.sh test
    else
        # Fallback to cargo test
        cargo test --all-features --all-targets
    fi
    
    log_success "All tests completed"
}

run_unit_tests() {
    log_info "Running unit tests..."
    cd "$PROJECT_ROOT"
    
    cargo test --lib --all-features
    
    log_success "Unit tests completed"
}

run_integration_tests() {
    log_info "Running integration tests..."
    cd "$PROJECT_ROOT"
    
    cargo test --test '*' --all-features
    
    log_success "Integration tests completed"
}

run_network_tests() {
    log_info "Running network simulation tests..."
    cd "$PROJECT_ROOT"
    
    # Set up network environment if script exists
    if [[ -x "./scripts/docker-test.sh" ]]; then
        ./scripts/docker-test.sh network
    fi
    
    # Run network tests with Docker features
    cargo test -p network-sim --features docker
    
    log_success "Network tests completed"
}

lint_code() {
    log_info "Running code linting..."
    cd "$PROJECT_ROOT"
    
    # Run clippy with strict settings
    cargo clippy --all-features --all-targets -- -D warnings
    
    log_success "Linting completed"
}

format_code() {
    log_info "Formatting code..."
    cd "$PROJECT_ROOT"
    
    cargo fmt --all
    
    log_success "Code formatting completed"
}

clean_project() {
    log_info "Cleaning build artifacts..."
    cd "$PROJECT_ROOT"
    
    cargo clean
    
    # Clean any additional build directories
    if [[ -d "gstreamer/build" ]]; then
        rm -rf gstreamer/build
    fi
    
    log_success "Project cleaned"
}

generate_docs() {
    log_info "Generating documentation..."
    cd "$PROJECT_ROOT"
    
    # Generate docs with all features
    cargo doc --all-features --no-deps --open
    
    log_success "Documentation generated"
}

run_benchmarks() {
    log_info "Running benchmarks..."
    cd "$PROJECT_ROOT"
    
    # Run benchmarks if available
    if cargo test --benches --all-features &> /dev/null; then
        cargo test --benches --all-features
    else
        log_warning "No benchmarks found"
    fi
    
    log_success "Benchmarks completed"
}

inspect_gstreamer() {
    log_info "Inspecting GStreamer plugins..."
    
    echo "=== Available RIST plugins ==="
    gst-inspect-1.0 | grep -i rist || echo "No RIST plugins found"
    
    echo -e "\n=== RIST source plugin details ==="
    gst-inspect-1.0 ristsrc 2>/dev/null || echo "ristsrc plugin not available"
    
    echo -e "\n=== RIST sink plugin details ==="
    gst-inspect-1.0 ristsink 2>/dev/null || echo "ristsink plugin not available"
    
    echo -e "\n=== Custom elements ==="
    echo "Checking for dispatcher and dynbitrate elements..."
    # Note: These are our custom elements that may not be installed system-wide
    
    log_success "GStreamer inspection completed"
}

check_network() {
    log_info "Checking network capabilities..."
    
    # Check if we can create network namespaces
    if ip netns add test-check 2>/dev/null; then
        ip netns del test-check
        log_success "Network namespace creation: OK"
    else
        log_warning "Network namespace creation: FAILED (requires NET_ADMIN capability)"
    fi
    
    # Check if we can modify traffic control
    if tc qdisc show &>/dev/null; then
        log_success "Traffic control access: OK"
    else
        log_warning "Traffic control access: FAILED"
    fi
    
    # Check available network interfaces
    echo -e "\n=== Available network interfaces ==="
    ip link show
    
    log_success "Network check completed"
}

# Main command handling
case "${1:-help}" in
    setup)
        setup_environment
        ;;
    build)
        build_project
        ;;
    test)
        run_tests
        ;;
    test-unit)
        run_unit_tests
        ;;
    test-integration)
        run_integration_tests
        ;;
    test-network)
        run_network_tests
        ;;
    lint)
        lint_code
        ;;
    format)
        format_code
        ;;
    clean)
        clean_project
        ;;
    docs)
        generate_docs
        ;;
    benchmark)
        run_benchmarks
        ;;
    gst-inspect)
        inspect_gstreamer
        ;;
    network-check)
        check_network
        ;;
    help|--help|-h)
        print_help
        ;;
    *)
        log_error "Unknown command: $1"
        echo
        print_help
        exit 1
        ;;
esac