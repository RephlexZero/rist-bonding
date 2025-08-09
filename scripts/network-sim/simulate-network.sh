#!/bin/bash

# Network Simulation Script
set -e

NETWORK_TYPE=${1:-"good-4g"}
LATENCY_MS=${LATENCY_MS:-50}
LOSS_PCT=${LOSS_PCT:-1.0}
BANDWIDTH_MBPS=${BANDWIDTH_MBPS:-20}
JITTER_MS=${JITTER_MS:-5}

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

# Variable network conditions (changes over time)
apply_variable_conditions() {
    local interface=${1:-eth0}
    
    while true; do
        # Generate random variations
        local current_latency=$((LATENCY_MS + (RANDOM % 100) - 50))
        local current_loss=$(echo "$LOSS_PCT + (($RANDOM % 100) - 50) * 0.1" | bc -l)
        local current_bw=$((BANDWIDTH_MBPS + (RANDOM % 20) - 10))
        
        # Ensure values are within reasonable bounds
        current_latency=$(( current_latency < 10 ? 10 : current_latency ))
        current_loss=$(echo "if ($current_loss < 0) 0 else if ($current_loss > 20) 20 else $current_loss" | bc -l)
        current_bw=$(( current_bw < 1 ? 1 : current_bw ))
        
        echo "Updating conditions: ${current_latency}ms, ${current_loss}%, ${current_bw}Mbps"
        
        # Clear and reapply
        tc qdisc del dev $interface root 2>/dev/null || true
        tc qdisc add dev $interface root handle 1: htb default 30
        tc class add dev $interface parent 1: classid 1:1 htb rate ${current_bw}mbit
        tc class add dev $interface parent 1:1 classid 1:30 htb rate ${current_bw}mbit
        tc qdisc add dev $interface parent 1:30 handle 30: netem \
            delay ${current_latency}ms ${JITTER_MS}ms \
            loss ${current_loss}%
        
        sleep 30  # Change conditions every 30 seconds
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
    "variable")
        echo "Starting variable network conditions"
        apply_variable_conditions $INTERFACE
        ;;
    *)
        echo "Applying static network conditions"
        apply_network_conditions $INTERFACE
        ;;
esac

# Keep the container running
while true; do
    sleep 60
    echo "Network simulation active: $NETWORK_TYPE"
done