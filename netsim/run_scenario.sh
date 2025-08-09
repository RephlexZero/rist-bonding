#!/bin/bash
# Main scenario runner script - orchestrates network setup, pipeline execution, and cleanup

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

# Configuration
NS_SENDER="ns_sender"
NS_RECEIVER="ns_receiver"
GST_PLUGIN_PATH="$ROOT_DIR/target/release"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() {
    echo -e "${GREEN}[SCENARIO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[SCENARIO]${NC} $1"
}

log_error() {
    echo -e "${RED}[SCENARIO]${NC} $1"
}

log_step() {
    echo -e "${BLUE}[STEP]${NC} $1"
}

cleanup_on_exit() {
    log_info "Cleaning up on exit..."
    
    # Kill any running pipelines
    if [ -n "${SENDER_PID:-}" ]; then
        kill "$SENDER_PID" 2>/dev/null || true
        wait "$SENDER_PID" 2>/dev/null || true
    fi
    
    if [ -n "${RECEIVER_PID:-}" ]; then
        kill "$RECEIVER_PID" 2>/dev/null || true
        wait "$RECEIVER_PID" 2>/dev/null || true
    fi
    
    # Clean up network
    "$SCRIPT_DIR/setup_network.sh" cleanup
    
    log_info "Cleanup completed"
}

trap cleanup_on_exit EXIT INT TERM

run_scenario() {
    local scenario_name="$1"
    local duration_multiplier="${2:-1}"
    
    log_info "Starting scenario: $scenario_name (duration multiplier: $duration_multiplier)"
    
    # Setup directories
    local results_dir="$ROOT_DIR/results/$scenario_name"
    local logs_dir="$ROOT_DIR/logs/$scenario_name"
    mkdir -p "$results_dir" "$logs_dir"
    
    # Load scenario configuration
    local scenario_file="$ROOT_DIR/scenarios/${scenario_name}.yaml"
    if [ ! -f "$scenario_file" ]; then
        log_error "Scenario file not found: $scenario_file"
        return 1
    fi
    
    log_step "Setting up network simulation..."
    "$SCRIPT_DIR/setup_network.sh" setup "$results_dir"
    
    log_step "Starting receiver pipeline..."
    start_receiver_pipeline "$logs_dir" &
    RECEIVER_PID=$!
    
    # Give receiver time to start
    sleep 2
    
    log_step "Starting sender pipeline..."
    start_sender_pipeline "$logs_dir" &
    SENDER_PID=$!
    
    # Give sender time to establish connection
    sleep 3
    
    log_step "Executing scenario schedule..."
    execute_scenario_schedule "$scenario_file" "$duration_multiplier" "$results_dir"
    
    log_step "Stopping pipelines..."
    if [ -n "${SENDER_PID:-}" ]; then
        kill "$SENDER_PID" 2>/dev/null || true
        wait "$SENDER_PID" 2>/dev/null || true
        unset SENDER_PID
    fi
    
    if [ -n "${RECEIVER_PID:-}" ]; then
        kill "$RECEIVER_PID" 2>/dev/null || true
        wait "$RECEIVER_PID" 2>/dev/null || true
        unset RECEIVER_PID
    fi
    
    log_step "Capturing final statistics..."
    "$SCRIPT_DIR/tc_control.sh" stats "$results_dir/final_stats.txt"
    
    log_step "Processing metrics..."
    python3 "$ROOT_DIR/metrics/process_scenario.py" "$scenario_name" "$results_dir" "$logs_dir"
    
    log_info "Scenario $scenario_name completed"
}

start_receiver_pipeline() {
    local logs_dir="$1"
    local log_file="$logs_dir/receiver.log"
    
    # Receiver pipeline: 4 RIST inputs → aggregation → metrics sink
    # Using ristsrc for each input port, then combine via queue → fakesink for now
    
    ip netns exec "$NS_RECEIVER" env GST_PLUGIN_PATH="$GST_PLUGIN_PATH" GST_DEBUG="rist*:4,dispatcher*:3" \
        gst-launch-1.0 -v \
        ristsrc address=10.0.1.2 port=5001 ! "queue name=q1 ! tee name=t" \
        ristsrc address=10.0.2.2 port=5002 ! "queue name=q2 ! t." \
        ristsrc address=10.0.3.2 port=5003 ! "queue name=q3 ! t." \
        ristsrc address=10.0.4.2 port=5004 ! "queue name=q4 ! t." \
        t. ! "queue ! fakesink name=sink dump=false" \
        > "$log_file" 2>&1
}

start_sender_pipeline() {
    local logs_dir="$1"
    local log_file="$logs_dir/sender.log"
    
    # Sender pipeline: test source → encode → ristdispatcher → 4 RIST outputs
    # Using videotestsrc for deterministic content
    
    ip netns exec "$NS_SENDER" env GST_PLUGIN_PATH="$GST_PLUGIN_PATH" GST_DEBUG="rist*:4,dispatcher*:3" \
        gst-launch-1.0 -v \
        videotestsrc pattern=ball is-live=true ! \
        "video/x-raw,width=640,height=480,framerate=30/1" ! \
        x264enc bitrate=2500 key-int-max=60 ! \
        "video/x-h264,profile=baseline" ! \
        rtph264pay ! \
        ristdispatcher name=disp \
        disp.src_0 ! ristsink address=10.0.1.2 port=5001 \
        disp.src_1 ! ristsink address=10.0.2.2 port=5002 \
        disp.src_2 ! ristsink address=10.0.3.2 port=5003 \
        disp.src_3 ! ristsink address=10.0.4.2 port=5004 \
        > "$log_file" 2>&1
}

execute_scenario_schedule() {
    local scenario_file="$1"
    local duration_multiplier="$2"
    local results_dir="$3"
    
    # For now, implement basic scenario execution
    # This should be expanded to parse YAML and execute precise timing
    
    local scenario_name=$(basename "$scenario_file" .yaml)
    local base_duration=30
    local duration=$((base_duration * duration_multiplier))
    
    log_info "Executing $scenario_name for ${duration}s with schedule patterns..."
    
    # Capture stats periodically
    local stats_interval=5
    local end_time=$(($(date +%s) + duration))
    local stats_count=0
    
    case "$scenario_name" in
        S0)
            execute_s0_baseline "$duration" "$results_dir"
            ;;
        S1)
            execute_s1_variable_bandwidth "$duration" "$results_dir"
            ;;
        S2)
            execute_s2_burst_loss "$duration" "$results_dir"
            ;;
        S3)
            execute_s3_link_outage "$duration" "$results_dir"
            ;;
        S4)
            execute_s4_asymmetric_latency "$duration" "$results_dir"
            ;;
        *)
            log_warn "Unknown scenario: $scenario_name, running baseline"
            execute_s0_baseline "$duration" "$results_dir"
            ;;
    esac
}

execute_s0_baseline() {
    local duration="$1"
    local results_dir="$2"
    
    log_info "S0 Baseline: All links at 1500kbps, 15ms latency, 0.5% loss"
    
    # Set all links to baseline configuration
    for i in {1..4}; do
        "$SCRIPT_DIR/tc_control.sh" link "$i" 1500 15 5 0.5
    done
    
    # Capture periodic stats
    capture_periodic_stats "$duration" "$results_dir" "baseline"
}

execute_s1_variable_bandwidth() {
    local duration="$1"
    local results_dir="$2"
    
    log_info "S1 Variable Bandwidth: Staggered bandwidth changes"
    
    # Initial state: all at 1500kbps
    for i in {1..4}; do
        "$SCRIPT_DIR/tc_control.sh" link "$i" 1500 20 5 1.0
    done
    
    # Schedule bandwidth changes every 10s (scaled by duration)
    local interval=$((duration / 6))  # 6 changes over the duration
    
    # Start background scheduler
    (
        sleep "$interval"
        log_info "S1: Link 1: 1500→750 kbps"
        "$SCRIPT_DIR/tc_control.sh" bandwidth 1 750
        
        sleep "$interval"
        log_info "S1: Link 2: 1500→250 kbps"
        "$SCRIPT_DIR/tc_control.sh" bandwidth 2 250
        
        sleep "$interval"
        log_info "S1: Link 1: 750→250 kbps"
        "$SCRIPT_DIR/tc_control.sh" bandwidth 1 250
        
        sleep "$interval"
        log_info "S1: Link 3: 1500→750 kbps"
        "$SCRIPT_DIR/tc_control.sh" bandwidth 3 750
        
        sleep "$interval"
        log_info "S1: Link 2: 250→1500 kbps"
        "$SCRIPT_DIR/tc_control.sh" bandwidth 2 1500
        
        sleep "$interval"
        log_info "S1: All links back to 1500 kbps"
        for i in {1..4}; do
            "$SCRIPT_DIR/tc_control.sh" bandwidth "$i" 1500
        done
    ) &
    
    capture_periodic_stats "$duration" "$results_dir" "variable_bw"
}

execute_s2_burst_loss() {
    local duration="$1"
    local results_dir="$2"
    
    log_info "S2 Burst Loss: Periodic loss bursts on rotating links"
    
    # Initial state: all at good conditions
    for i in {1..4}; do
        "$SCRIPT_DIR/tc_control.sh" link "$i" 1200 25 8 1.0
    done
    
    # Apply burst loss every 8 seconds, rotating between links
    local burst_interval=$((duration / 8))
    
    (
        for round in {1..8}; do
            sleep "$burst_interval"
            local link=$(((round - 1) % 4 + 1))
            log_info "S2: Applying 25% burst loss to link $link for 3s"
            "$SCRIPT_DIR/tc_control.sh" burst "$link" 25 3000
        done
    ) &
    
    capture_periodic_stats "$duration" "$results_dir" "burst_loss"
}

execute_s3_link_outage() {
    local duration="$1"
    local results_dir="$2"
    
    log_info "S3 Link Outage: Sequential link outages and restoration"
    
    # Initial state
    for i in {1..4}; do
        "$SCRIPT_DIR/tc_control.sh" link "$i" 1000 30 10 0.8
    done
    
    local outage_duration=10
    local recovery_time=5
    local cycle_time=$((outage_duration + recovery_time))
    
    (
        for link in {1..4}; do
            if [ $(($(date +%s) - start_time)) -ge $duration ]; then break; fi
            
            log_info "S3: Link $link outage for ${outage_duration}s"
            "$SCRIPT_DIR/tc_control.sh" outage "$link"
            
            sleep "$outage_duration"
            
            log_info "S3: Link $link restored"
            "$SCRIPT_DIR/tc_control.sh" restore "$link" 1000
            
            sleep "$recovery_time"
        done
    ) &
    
    local start_time=$(date +%s)
    capture_periodic_stats "$duration" "$results_dir" "outage"
}

execute_s4_asymmetric_latency() {
    local duration="$1"
    local results_dir="$2"
    
    log_info "S4 Asymmetric Latency: Different latencies and capacities per link"
    
    # Set asymmetric configuration per the roadmap
    "$SCRIPT_DIR/tc_control.sh" link 1 1500 10 5 0.5   # Fast, low latency
    "$SCRIPT_DIR/tc_control.sh" link 2 1000 40 10 1.0  # Medium
    "$SCRIPT_DIR/tc_control.sh" link 3 500 80 15 1.5   # Slower, higher latency
    "$SCRIPT_DIR/tc_control.sh" link 4 250 120 20 2.0  # Slowest, highest latency
    
    capture_periodic_stats "$duration" "$results_dir" "asymmetric"
}

capture_periodic_stats() {
    local duration="$1"
    local results_dir="$2"
    local prefix="$3"
    
    local end_time=$(($(date +%s) + duration))
    local stats_interval=5
    local count=0
    
    while [ $(date +%s) -lt "$end_time" ]; do
        sleep "$stats_interval"
        count=$((count + 1))
        "$SCRIPT_DIR/tc_control.sh" stats "$results_dir/${prefix}_stats_${count}.txt"
    done
}

main() {
    if [ $# -eq 0 ]; then
        echo "Usage: $0 <scenario_name> [duration_multiplier]"
        echo "Available scenarios: S0, S1, S2, S3, S4"
        echo "Duration multiplier: scales test duration (default: 1)"
        exit 1
    fi
    
    local scenario_name="$1"
    local duration_multiplier="${2:-1}"
    
    # Verify plugin exists
    if [ ! -f "$GST_PLUGIN_PATH/libgstristsmart.so" ]; then
        log_error "Plugin not found at $GST_PLUGIN_PATH/libgstristsmart.so"
        log_error "Please run: PKG_CONFIG_PATH=/usr/lib/pkgconfig cargo build --release"
        exit 1
    fi
    
    run_scenario "$scenario_name" "$duration_multiplier"
}

# Only execute main if script is run directly
if [ "${BASH_SOURCE[0]}" = "${0}" ]; then
    main "$@"
fi
