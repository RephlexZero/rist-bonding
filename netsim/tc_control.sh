#!/bin/bash
# Traffic control utilities for dynamic network shaping during scenarios

set -euo pipefail

# Configuration
NS_SENDER="ns_sender"
NUM_LINKS=4

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log_info() {
    echo -e "${GREEN}[TC]${NC} $1"
}

log_debug() {
    echo -e "${YELLOW}[TC-DEBUG]${NC} $1"
}

# Update bandwidth for a specific link
# Usage: update_bandwidth <link_id> <rate_kbps>
update_bandwidth() {
    local link_id="$1"
    local rate_kbps="$2"
    local dev="vethS${link_id}"
    
    if [ "$rate_kbps" -eq 0 ]; then
        # Special case: 0 means link outage - set to minimal rate and high loss
        rate_kbps=1
        ip netns exec "$NS_SENDER" tc qdisc change dev "$dev" parent 1:10 handle 10: netem delay 100ms 20ms loss 100%
        log_info "Link $link_id: OUTAGE (100% loss)"
    else
        # Update HTB rate
        ip netns exec "$NS_SENDER" tc class change dev "$dev" parent 1: classid 1:10 htb rate "${rate_kbps}kbit" ceil "${rate_kbps}kbit"
        log_info "Link $link_id: bandwidth updated to ${rate_kbps} kbps"
    fi
}

# Update latency for a specific link
# Usage: update_latency <link_id> <delay_ms> <jitter_ms>
update_latency() {
    local link_id="$1"
    local delay_ms="$2"
    local jitter_ms="${3:-5}"
    local dev="vethS${link_id}"
    
    # Get current loss rate (preserve it)
    local current_loss=$(ip netns exec "$NS_SENDER" tc qdisc show dev "$dev" | grep -oE 'loss [0-9.]+%' | cut -d' ' -f2 || echo "0.5%")
    
    ip netns exec "$NS_SENDER" tc qdisc change dev "$dev" parent 1:10 handle 10: netem delay "${delay_ms}ms" "${jitter_ms}ms" loss "$current_loss"
    log_info "Link $link_id: latency updated to ${delay_ms}ms ±${jitter_ms}ms"
}

# Update loss rate for a specific link
# Usage: update_loss <link_id> <loss_pct>
update_loss() {
    local link_id="$1"
    local loss_pct="$2"
    local dev="vethS${link_id}"
    
    # Get current delay settings (preserve them)
    local delay_info=$(ip netns exec "$NS_SENDER" tc qdisc show dev "$dev" | grep -oE 'delay [0-9.]+ms( [0-9.]+ms)?' || echo "delay 20ms 5ms")
    
    ip netns exec "$NS_SENDER" tc qdisc change dev "$dev" parent 1:10 handle 10: netem $delay_info loss "${loss_pct}%"
    log_info "Link $link_id: loss updated to ${loss_pct}%"
}

# Update all parameters for a link at once
# Usage: update_link <link_id> <rate_kbps> <delay_ms> <jitter_ms> <loss_pct>
update_link() {
    local link_id="$1"
    local rate_kbps="$2"
    local delay_ms="$3"
    local jitter_ms="$4"
    local loss_pct="$5"
    local dev="vethS${link_id}"
    
    if [ "$rate_kbps" -eq 0 ]; then
        # Outage case
        update_bandwidth "$link_id" 0
    else
        # Update bandwidth
        ip netns exec "$NS_SENDER" tc class change dev "$dev" parent 1: classid 1:10 htb rate "${rate_kbps}kbit" ceil "${rate_kbps}kbit"
        
        # Update netem parameters
        ip netns exec "$NS_SENDER" tc qdisc change dev "$dev" parent 1:10 handle 10: netem delay "${delay_ms}ms" "${jitter_ms}ms" loss "${loss_pct}%"
        
        log_info "Link $link_id: updated to ${rate_kbps}kbps, ${delay_ms}±${jitter_ms}ms, ${loss_pct}% loss"
    fi
}

# Capture current tc statistics for all links
# Usage: capture_stats <output_file>
capture_stats() {
    local output_file="$1"
    local timestamp=$(date '+%Y-%m-%d %H:%M:%S')
    
    {
        echo "=== TC Statistics Snapshot at $timestamp ==="
        echo ""
        
        for i in $(seq 1 $NUM_LINKS); do
            echo "--- Link $i (vethS${i}) ---"
            echo "HTB Classes:"
            ip netns exec "$NS_SENDER" tc -s class show dev "vethS${i}" 2>/dev/null || echo "  No HTB classes found"
            echo ""
            echo "Queueing Disciplines:"
            ip netns exec "$NS_SENDER" tc -s qdisc show dev "vethS${i}" 2>/dev/null || echo "  No qdiscs found"
            echo ""
            echo "Interface Statistics:"
            ip netns exec "$NS_SENDER" ip -s link show "vethS${i}" 2>/dev/null || echo "  Interface not found"
            echo ""
            echo "----------------------------------------"
            echo ""
        done
    } > "$output_file"
    
    log_info "Statistics captured to $output_file"
}

# Apply a burst loss pattern to a link
# Usage: apply_burst_loss <link_id> <burst_loss_pct> <duration_ms>
apply_burst_loss() {
    local link_id="$1"
    local burst_loss_pct="$2"
    local duration_ms="$3"
    local dev="vethS${link_id}"
    
    # Get current settings to restore later
    local delay_info=$(ip netns exec "$NS_SENDER" tc qdisc show dev "$dev" | grep -oE 'delay [0-9.]+ms( [0-9.]+ms)?' || echo "delay 20ms 5ms")
    local original_loss=$(ip netns exec "$NS_SENDER" tc qdisc show dev "$dev" | grep -oE 'loss [0-9.]+%' | cut -d' ' -f2 | tr -d '%' || echo "0.5")
    
    # Apply burst loss
    ip netns exec "$NS_SENDER" tc qdisc change dev "$dev" parent 1:10 handle 10: netem $delay_info loss "${burst_loss_pct}%"
    log_info "Link $link_id: applying ${burst_loss_pct}% burst loss for ${duration_ms}ms"
    
    # Schedule restoration (background job)
    (
        sleep $(echo "$duration_ms / 1000" | bc -l)
        ip netns exec "$NS_SENDER" tc qdisc change dev "$dev" parent 1:10 handle 10: netem $delay_info loss "${original_loss}%"
        log_info "Link $link_id: burst loss ended, restored to ${original_loss}%"
    ) &
}

# Get current configuration for a link (for verification)
# Usage: get_link_config <link_id>
get_link_config() {
    local link_id="$1"
    local dev="vethS${link_id}"
    
    echo "=== Link $link_id Configuration ==="
    echo "HTB Rate Limiting:"
    ip netns exec "$NS_SENDER" tc class show dev "$dev" | grep "rate" || echo "  No rate limiting found"
    echo "Netem Parameters:"
    ip netns exec "$NS_SENDER" tc qdisc show dev "$dev" | grep "netem" || echo "  No netem found"
    echo ""
}

# Show all current configurations
show_all_configs() {
    for i in $(seq 1 $NUM_LINKS); do
        get_link_config "$i"
    done
}

# Parse command line and execute
main() {
    if [ $# -eq 0 ]; then
        echo "Usage: $0 <command> [args...]"
        echo "Commands:"
        echo "  bandwidth <link_id> <rate_kbps>                     - Update bandwidth"
        echo "  latency <link_id> <delay_ms> [jitter_ms]            - Update latency"
        echo "  loss <link_id> <loss_pct>                           - Update loss rate"
        echo "  link <link_id> <rate_kbps> <delay_ms> <jitter_ms> <loss_pct>  - Update all"
        echo "  burst <link_id> <loss_pct> <duration_ms>            - Apply burst loss"
        echo "  stats <output_file>                                 - Capture statistics"
        echo "  show [link_id]                                      - Show configuration"
        echo "  outage <link_id>                                    - Simulate link outage"
        echo "  restore <link_id> <rate_kbps>                       - Restore from outage"
        exit 1
    fi
    
    local command="$1"
    shift
    
    case "$command" in
        bandwidth)
            update_bandwidth "$1" "$2"
            ;;
        latency)
            update_latency "$1" "$2" "${3:-5}"
            ;;
        loss)
            update_loss "$1" "$2"
            ;;
        link)
            update_link "$1" "$2" "$3" "$4" "$5"
            ;;
        burst)
            apply_burst_loss "$1" "$2" "$3"
            ;;
        stats)
            capture_stats "$1"
            ;;
        show)
            if [ $# -gt 0 ]; then
                get_link_config "$1"
            else
                show_all_configs
            fi
            ;;
        outage)
            update_bandwidth "$1" 0
            ;;
        restore)
            update_bandwidth "$1" "$2"
            ;;
        *)
            echo "Unknown command: $command"
            exit 1
            ;;
    esac
}

# Only execute main if script is run directly
if [ "${BASH_SOURCE[0]}" = "${0}" ]; then
    main "$@"
fi
