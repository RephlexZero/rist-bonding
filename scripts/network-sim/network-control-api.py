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

# Current network state
current_state = {
    'latency_ms': 50,
    'loss_pct': 1.0,
    'bandwidth_mbps': 20,
    'jitter_ms': 5,
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
    """Apply network preset"""
    presets = {
        'good-4g': {'latency_ms': 50, 'loss_pct': 1.0, 'bandwidth_mbps': 20, 'jitter_ms': 5},
        'poor-4g': {'latency_ms': 150, 'loss_pct': 5.0, 'bandwidth_mbps': 10, 'jitter_ms': 20},
        '5g': {'latency_ms': 20, 'loss_pct': 0.1, 'bandwidth_mbps': 100, 'jitter_ms': 2},
        'variable': {'latency_ms': 75, 'loss_pct': 2.0, 'bandwidth_mbps': 30, 'jitter_ms': 10}
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