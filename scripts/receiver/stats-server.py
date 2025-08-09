#!/usr/bin/env python3

"""
Statistics collection server for RIST receiver
"""

from flask import Flask, jsonify
import json
import time
import threading
import psutil
import os

app = Flask(__name__)

# Global statistics storage
stats_data = {
    'start_time': time.time(),
    'packets_received': 0,
    'packets_lost': 0,
    'bytes_received': 0,
    'last_update': time.time(),
    'network_stats': {},
    'system_stats': {}
}

def collect_system_stats():
    """Collect system performance statistics"""
    while True:
        try:
            cpu_percent = psutil.cpu_percent(interval=1)
            memory = psutil.virtual_memory()
            
            stats_data['system_stats'] = {
                'cpu_percent': cpu_percent,
                'memory_percent': memory.percent,
                'memory_used_mb': memory.used / (1024 * 1024),
                'timestamp': time.time()
            }
            
            time.sleep(5)  # Update every 5 seconds
        except Exception as e:
            print(f"Error collecting system stats: {e}")
            time.sleep(5)

@app.route('/stats')
def get_stats():
    """Return current statistics"""
    current_time = time.time()
    uptime = current_time - stats_data['start_time']
    
    response = {
        'uptime_seconds': uptime,
        'packets_received': stats_data['packets_received'],
        'packets_lost': stats_data['packets_lost'],
        'bytes_received': stats_data['bytes_received'],
        'packet_loss_rate': (
            stats_data['packets_lost'] / max(stats_data['packets_received'], 1) * 100
        ),
        'throughput_mbps': (
            stats_data['bytes_received'] * 8 / (1024 * 1024) / max(uptime, 1)
        ),
        'system_stats': stats_data.get('system_stats', {}),
        'network_stats': stats_data.get('network_stats', {}),
        'timestamp': current_time
    }
    
    return jsonify(response)

@app.route('/health')
def health_check():
    """Health check endpoint"""
    return jsonify({'status': 'healthy', 'timestamp': time.time()})

@app.route('/reset')
def reset_stats():
    """Reset statistics"""
    global stats_data
    stats_data = {
        'start_time': time.time(),
        'packets_received': 0,
        'packets_lost': 0,
        'bytes_received': 0,
        'last_update': time.time(),
        'network_stats': {},
        'system_stats': {}
    }
    return jsonify({'status': 'reset', 'timestamp': time.time()})

def main():
    print("Starting RIST statistics server on port 8080...")
    
    # Start system stats collection in background
    stats_thread = threading.Thread(target=collect_system_stats, daemon=True)
    stats_thread.start()
    
    # Start Flask server
    app.run(host='0.0.0.0', port=8080, debug=False)

if __name__ == "__main__":
    main()