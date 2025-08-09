#!/bin/bash
# Network simulation setup script
# Creates namespaces, veth pairs, and applies tc shaping

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

# Configuration
NS_SENDER="ns_sender"
NS_RECEIVER="ns_receiver"
NUM_LINKS=4

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

cleanup_existing() {
    log_info "Cleaning up existing network configuration..."
    
    # Remove existing namespaces (this will also clean up veth pairs)
    for ns in "$NS_SENDER" "$NS_RECEIVER"; do
        if ip netns list | grep -q "^${ns}"; then
            ip netns delete "$ns" 2>/dev/null || true
            log_info "Removed namespace: $ns"
        fi
    done
    
    # Remove any orphaned veth pairs
    for i in $(seq 1 $NUM_LINKS); do
        ip link delete "vethS${i}" 2>/dev/null || true
        ip link delete "vethR${i}" 2>/dev/null || true
    done
    
    log_info "Cleanup completed"
}

create_namespaces() {
    log_info "Creating network namespaces..."
    
    # Create namespaces
    ip netns add "$NS_SENDER"
    ip netns add "$NS_RECEIVER"
    
    # Enable loopback interfaces
    ip netns exec "$NS_SENDER" ip link set lo up
    ip netns exec "$NS_RECEIVER" ip link set lo up
    
    log_info "Created namespaces: $NS_SENDER, $NS_RECEIVER"
}

create_veth_pairs() {
    log_info "Creating veth pairs and configuring links..."
    
    for i in $(seq 1 $NUM_LINKS); do
        # Create veth pair
        ip link add "vethS${i}" type veth peer name "vethR${i}"
        
        # Move to respective namespaces
        ip link set "vethS${i}" netns "$NS_SENDER"
        ip link set "vethR${i}" netns "$NS_RECEIVER"
        
        # Configure IP addresses (10.0.i.1/30 and 10.0.i.2/30)
        ip netns exec "$NS_SENDER" ip addr add "10.0.${i}.1/30" dev "vethS${i}"
        ip netns exec "$NS_RECEIVER" ip addr add "10.0.${i}.2/30" dev "vethR${i}"
        
        # Bring interfaces up
        ip netns exec "$NS_SENDER" ip link set "vethS${i}" up
        ip netns exec "$NS_RECEIVER" ip link set "vethR${i}" up
        
        log_info "Created link $i: vethS${i} (10.0.${i}.1) <-> vethR${i} (10.0.${i}.2)"
    done
}

setup_routing() {
    log_info "Setting up routing tables..."
    
    # Add routes in sender namespace to reach each receiver IP via corresponding link
    for i in $(seq 1 $NUM_LINKS); do
        ip netns exec "$NS_SENDER" ip route add "10.0.${i}.2/32" dev "vethS${i}" src "10.0.${i}.1"
    done
    
    # Add routes in receiver namespace (usually not needed but for completeness)
    for i in $(seq 1 $NUM_LINKS); do
        ip netns exec "$NS_RECEIVER" ip route add "10.0.${i}.1/32" dev "vethR${i}" src "10.0.${i}.2"
    done
    
    log_info "Routing tables configured"
}

apply_initial_shaping() {
    log_info "Applying initial tc shaping (default rates)..."
    
    for i in $(seq 1 $NUM_LINKS); do
        # Apply to sender-side interface (outgoing traffic shaping)
        ip netns exec "$NS_SENDER" tc qdisc add dev "vethS${i}" root handle 1: htb default 10
        ip netns exec "$NS_SENDER" tc class add dev "vethS${i}" parent 1: classid 1:10 htb rate 1500kbit ceil 1500kbit
        
        # Add netem for latency/jitter/loss
        ip netns exec "$NS_SENDER" tc qdisc add dev "vethS${i}" parent 1:10 handle 10: netem delay 20ms 5ms loss 0.5%
        
        log_info "Applied initial shaping to link $i (1500 kbit/s, 20ms delay, 0.5% loss)"
    done
}

capture_initial_stats() {
    local stats_dir="$1"
    mkdir -p "$stats_dir"
    
    log_info "Capturing initial network statistics..."
    
    # Capture tc statistics
    for i in $(seq 1 $NUM_LINKS); do
        {
            echo "=== Link $i Sender Side ==="
            ip netns exec "$NS_SENDER" tc -s qdisc show dev "vethS${i}" || true
            echo "=== Link $i Interface Stats ==="
            ip netns exec "$NS_SENDER" ip -s link show "vethS${i}" || true
            echo ""
        } > "$stats_dir/initial_link_${i}.txt"
    done
    
    # Capture routing tables
    {
        echo "=== Sender Routing Table ==="
        ip netns exec "$NS_SENDER" ip route show
        echo "=== Receiver Routing Table ==="
        ip netns exec "$NS_RECEIVER" ip route show
    } > "$stats_dir/initial_routes.txt"
    
    log_info "Initial stats captured to $stats_dir"
}

verify_setup() {
    log_info "Verifying network setup..."
    
    local success=true
    
    # Test connectivity on each link
    for i in $(seq 1 $NUM_LINKS); do
        if ip netns exec "$NS_SENDER" ping -c 1 -W 1 "10.0.${i}.2" >/dev/null 2>&1; then
            log_info "Link $i connectivity: OK"
        else
            log_error "Link $i connectivity: FAILED"
            success=false
        fi
    done
    
    if [ "$success" = true ]; then
        log_info "Network setup verification: PASSED"
        return 0
    else
        log_error "Network setup verification: FAILED"
        return 1
    fi
}

print_usage() {
    echo "Usage: $0 [setup|cleanup|verify] [stats_dir]"
    echo "  setup:   Create and configure network simulation"
    echo "  cleanup: Remove network simulation"
    echo "  verify:  Test connectivity"
    echo "  stats_dir: Directory to save initial statistics (optional, for setup)"
}

main() {
    local action="${1:-setup}"
    local stats_dir="${2:-}"
    
    case "$action" in
        setup)
            if [ "$(id -u)" -ne 0 ]; then
                log_error "This script requires root privileges for network setup"
                exit 1
            fi
            
            cleanup_existing
            create_namespaces
            create_veth_pairs
            setup_routing
            apply_initial_shaping
            
            if [ -n "$stats_dir" ]; then
                capture_initial_stats "$stats_dir"
            fi
            
            verify_setup
            log_info "Network simulation setup completed successfully"
            ;;
        cleanup)
            if [ "$(id -u)" -ne 0 ]; then
                log_error "This script requires root privileges for cleanup"
                exit 1
            fi
            cleanup_existing
            ;;
        verify)
            verify_setup
            ;;
        *)
            print_usage
            exit 1
            ;;
    esac
}

# Only execute main if script is run directly (not sourced)
if [ "${BASH_SOURCE[0]}" = "${0}" ]; then
    main "$@"
fi
