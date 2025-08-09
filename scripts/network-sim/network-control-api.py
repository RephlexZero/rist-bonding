#!/usr/bin/env python3

"""
Network Control API for dynamic network simulation
"""

from flask import Flask, jsonify, request
import subprocess
import os
import time
import threading

app = Flask(__name__)

# Current network state with race track defaults
current_state = {
    'latency_ms': 80,
    'loss_pct': 2.0,
    'bandwidth_mbps': 7,
    'jitter_ms': 15,
    'interface': 'eth0',
    'last_update': time.time()
}

def apply_tc_rules(interface, latency, loss, bandwidth, jitter):
    """Apply traffic control rules"""
    try:
        # Clear existing rules
        subprocess.run(['tc', 'qdisc', 'del', 'dev', interface, 'root'], 
                      capture_output=True, check=False)
        
        # Apply new rules
        subprocess.run(['tc', 'qdisc', 'add', 'dev', interface, 'root', 'handle', '1:', 'htb', 'default', '30'], check=True)
        subprocess.run(['tc', 'class', 'add', 'dev', interface, 'parent', '1:', 'classid', '1:1', 'htb', 'rate', f'{bandwidth}mbit'], check=True)
        subprocess.run(['tc', 'class', 'add', 'dev', interface, 'parent', '1:1', 'classid', '1:30', 'htb', 'rate', f'{bandwidth}mbit'], check=True)
        subprocess.run(['tc', 'qdisc', 'add', 'dev', interface, 'parent', '1:30', 'handle', '30:', 'netem', 'delay', f'{latency}ms', f'{jitter}ms', 'loss', f'{loss}%'], check=True)
        
        return True
    except subprocess.CalledProcessError as e:
        print(f"Error applying tc rules: {e}")
        return False

@app.route('/status')
def get_status():
    """Get current network status"""
    return jsonify(current_state)

@app.route('/update', methods=['POST'])
def update_network():
    """Update network conditions"""
    data = request.get_json()
    
    # Update current state
    if 'latency_ms' in data:
        current_state['latency_ms'] = float(data['latency_ms'])
    if 'loss_pct' in data:
        current_state['loss_pct'] = float(data['loss_pct'])
    if 'bandwidth_mbps' in data:
        current_state['bandwidth_mbps'] = float(data['bandwidth_mbps'])
    if 'jitter_ms' in data:
        current_state['jitter_ms'] = float(data['jitter_ms'])
    
    # Apply the changes
    success = apply_tc_rules(
        current_state['interface'],
        current_state['latency_ms'],
        current_state['loss_pct'],
        current_state['bandwidth_mbps'],
        current_state['jitter_ms']
    )
    
    current_state['last_update'] = time.time()
    
    return jsonify({
        'success': success,
        'current_state': current_state
    })

@app.route('/preset/<preset_name>')
def apply_preset(preset_name):
    """Apply network preset for race track conditions"""
    presets = {
        # Race track specific profiles based on real-world data
        'race-track-best': {'latency_ms': 80, 'loss_pct': 2.0, 'bandwidth_mbps': 7, 'jitter_ms': 15},
        'race-track-variable-1': {'latency_ms': 120, 'loss_pct': 4.0, 'bandwidth_mbps': 1.5, 'jitter_ms': 25},
        'race-track-variable-2': {'latency_ms': 140, 'loss_pct': 6.0, 'bandwidth_mbps': 0.8, 'jitter_ms': 30},
        'race-track-variable-3': {'latency_ms': 160, 'loss_pct': 8.0, 'bandwidth_mbps': 0.4, 'jitter_ms': 40},
        # Legacy presets for backwards compatibility
        'good-4g': {'latency_ms': 80, 'loss_pct': 2.0, 'bandwidth_mbps': 7, 'jitter_ms': 15},  # Map to best race track
        'poor-4g': {'latency_ms': 120, 'loss_pct': 4.0, 'bandwidth_mbps': 1.5, 'jitter_ms': 25},  # Map to variable-1
        '5g': {'latency_ms': 80, 'loss_pct': 2.0, 'bandwidth_mbps': 7, 'jitter_ms': 15},  # Map to best race track
        'variable': {'latency_ms': 140, 'loss_pct': 6.0, 'bandwidth_mbps': 0.8, 'jitter_ms': 30}  # Map to variable-2
    }
    
    if preset_name not in presets:
        return jsonify({'error': 'Unknown preset'}), 400
    
    preset = presets[preset_name]
    
    # Update state
    current_state.update(preset)
    current_state['last_update'] = time.time()
    
    # Apply the changes
    success = apply_tc_rules(
        current_state['interface'],
        current_state['latency_ms'],
        current_state['loss_pct'],
        current_state['bandwidth_mbps'],
        current_state['jitter_ms']
    )
    
    return jsonify({
        'success': success,
        'preset_applied': preset_name,
        'current_state': current_state
    })

@app.route('/stats')
def get_network_stats():
    """Get network interface statistics"""
    interface = current_state['interface']
    
    try:
        # Get interface statistics
        result = subprocess.run(['cat', f'/proc/net/dev'], capture_output=True, text=True, check=True)
        lines = result.stdout.strip().split('\n')
        
        stats = {}
        for line in lines:
            if interface in line:
                parts = line.split()
                stats = {
                    'rx_bytes': int(parts[1]),
                    'rx_packets': int(parts[2]),
                    'rx_errors': int(parts[3]),
                    'rx_dropped': int(parts[4]),
                    'tx_bytes': int(parts[9]),
                    'tx_packets': int(parts[10]),
                    'tx_errors': int(parts[11]),
                    'tx_dropped': int(parts[12])
                }
                break
        
        return jsonify(stats)
    except Exception as e:
        return jsonify({'error': str(e)}), 500

def main():
    # Get network interface
    try:
        result = subprocess.run(['ip', 'route', 'get', '8.8.8.8'], capture_output=True, text=True, check=True)
        interface = result.stdout.split()[4]
        current_state['interface'] = interface
        print(f"Using network interface: {interface}")
    except Exception as e:
        print(f"Could not determine network interface, using eth0: {e}")
    
    print("Starting network control API on port 8090...")
    app.run(host='0.0.0.0', port=8090, debug=False)

if __name__ == "__main__":
    main()