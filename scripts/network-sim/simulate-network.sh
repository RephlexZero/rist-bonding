#!/bin/bash

# Network Simulation Script for Race Track Conditions
set -e

NETWORK_TYPE=${1:-"race-track-best"}
LATENCY_MS=${LATENCY_MS:-80}
LOSS_PCT=${LOSS_PCT:-2.0}
BANDWIDTH_MBPS=${BANDWIDTH_MBPS:-7}
JITTER_MS=${JITTER_MS:-15}

echo "Starting network simulation: $NETWORK_TYPE"
echo "  Latency: ${LATENCY_MS}ms"
echo "  Loss: ${LOSS_PCT}%"
echo "  Bandwidth: ${BANDWIDTH_MBPS}Mbps"
echo "  Jitter: ${JITTER_MS}ms"

# Apply network conditions using tc (traffic control)
apply_network_conditions() {
    local interface=${1:-eth0}
    
    echo "Applying network conditions to interface $interface"
    
    # Clear any existing rules
    tc qdisc del dev $interface root 2>/dev/null || true
    
    # Add root qdisc
    tc qdisc add dev $interface root handle 1: htb default 30
    
    # Add class for bandwidth limiting
    tc class add dev $interface parent 1: classid 1:1 htb rate ${BANDWIDTH_MBPS}mbit
    tc class add dev $interface parent 1:1 classid 1:30 htb rate ${BANDWIDTH_MBPS}mbit
    
    # Add netem for latency, jitter, and loss
    tc qdisc add dev $interface parent 1:30 handle 30: netem \
        delay ${LATENCY_MS}ms ${JITTER_MS}ms \
        loss ${LOSS_PCT}%
    
    echo "Network conditions applied successfully"
}

# Variable network conditions for race track (changes over time)
apply_race_track_variable_conditions() {
    local interface=${1:-eth0}
    local base_bw_kbps=$(echo "$BANDWIDTH_MBPS * 1000" | bc -l)
    
    while true; do
        # Generate variations within race track realistic ranges
        # Base bandwidth varies ±50% from configured value
        local bw_variation=$(echo "scale=0; ($RANDOM % 100) - 50" | bc)
        local current_bw_kbps=$(echo "scale=0; $base_bw_kbps * (100 + $bw_variation) / 100" | bc)
        
        # Ensure bandwidth stays within race track bounds (250kbps to 7000kbps)
        current_bw_kbps=$(echo "if ($current_bw_kbps < 250) 250 else if ($current_bw_kbps > 7000) 7000 else $current_bw_kbps" | bc)
        local current_bw_mbps=$(echo "scale=2; $current_bw_kbps / 1000" | bc)
        
        # Latency varies ±30% from base
        local latency_variation=$(echo "scale=0; ($RANDOM % 60) - 30" | bc)
        local current_latency=$(echo "scale=0; $LATENCY_MS * (100 + $latency_variation) / 100" | bc)
        current_latency=$(echo "if ($current_latency < 20) 20 else if ($current_latency > 300) 300 else $current_latency" | bc)
        
        # Loss varies ±50% from base
        local loss_variation=$(echo "scale=1; ($RANDOM % 100) - 50" | bc)
        local current_loss=$(echo "scale=1; $LOSS_PCT * (100 + $loss_variation) / 100" | bc)
        current_loss=$(echo "if ($current_loss < 0) 0 else if ($current_loss > 15) 15 else $current_loss" | bc)
        
        echo "Race track conditions: ${current_bw_kbps}kbps, ${current_latency}ms, ${current_loss}%, ${JITTER_MS}ms"
        
        # Clear and reapply
        tc qdisc del dev $interface root 2>/dev/null || true
        tc qdisc add dev $interface root handle 1: htb default 30
        tc class add dev $interface parent 1: classid 1:1 htb rate ${current_bw_mbps}mbit
        tc class add dev $interface parent 1:1 classid 1:30 htb rate ${current_bw_mbps}mbit
        tc qdisc add dev $interface parent 1:30 handle 30: netem \
            delay ${current_latency}ms ${JITTER_MS}ms \
            loss ${current_loss}%
        
        # Change conditions every 15-45 seconds to simulate moving through race track coverage areas
        local sleep_time=$((15 + RANDOM % 30))
        sleep $sleep_time
    done
}

# Start network control API
python3 scripts/network-control-api.py &
API_PID=$!

# Cleanup function
cleanup() {
    echo "Cleaning up network simulation..."
    kill $API_PID 2>/dev/null || true
    # Clear network rules
    for interface in $(ip link show | awk -F: '/^[0-9]+:/{print $2}' | tr -d ' '); do
        tc qdisc del dev $interface root 2>/dev/null || true
    done
    exit 0
}
trap cleanup SIGTERM SIGINT

# Find the main network interface
INTERFACE=$(ip route get 8.8.8.8 | awk '{print $5; exit}')
echo "Using network interface: $INTERFACE"

# Apply network conditions based on type
case "$NETWORK_TYPE" in
    "race-track-best")
        echo "Applying best race track link conditions (up to 7Mbps)"
        apply_network_conditions $INTERFACE
        ;;
    "race-track-variable-1"|"race-track-variable-2"|"race-track-variable-3")
        echo "Starting variable race track conditions for $NETWORK_TYPE"
        apply_race_track_variable_conditions $INTERFACE
        ;;
    "variable")
        echo "Starting legacy variable network conditions (for backwards compatibility)"
        apply_race_track_variable_conditions $INTERFACE
        ;;
    *)
        echo "Applying static race track conditions"
        apply_network_conditions $INTERFACE
        ;;
esac

# Keep the container running
while true; do
    sleep 60
    echo "Network simulation active: $NETWORK_TYPE"
done