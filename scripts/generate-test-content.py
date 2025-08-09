#!/usr/bin/env python3

"""
Generate test content for RIST bonding tests
"""

import os
import sys
import subprocess
from pathlib import Path

def generate_test_video():
    """Generate test video content"""
    test_data_dir = Path("test-data")
    test_data_dir.mkdir(exist_ok=True)
    
    # Generate a simple test pattern video
    print("Generating test video pattern...")
    
    cmd = [
        "gst-launch-1.0",
        "videotestsrc", "pattern=smpte", "num-buffers=900",  # 30 seconds at 30fps
        "!", "video/x-raw,width=1280,height=720,framerate=30/1",
        "!", "x264enc", "bitrate=2000", "tune=zerolatency",
        "!", "mp4mux",
        "!", "filesink", f"location={test_data_dir}/test-pattern.mp4"
    ]
    
    try:
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=60)
        if result.returncode == 0:
            print("✓ Test video generated successfully")
        else:
            print(f"⚠ Test video generation had issues: {result.stderr}")
    except subprocess.TimeoutExpired:
        print("⚠ Test video generation timed out")
    except Exception as e:
        print(f"⚠ Test video generation failed: {e}")

def generate_test_config():
    """Generate test configuration files"""
    config_dir = Path("test-data/configs")
    config_dir.mkdir(exist_ok=True, parents=True)
    
    # Basic bonding configuration
    config = {
        "bonding": {
            "links": [
                {"address": "172.20.1.2:5004", "weight": 1.0, "name": "good-4g"},
                {"address": "172.20.2.2:5005", "weight": 0.5, "name": "poor-4g"},
                {"address": "172.20.3.2:5006", "weight": 2.0, "name": "5g"},
                {"address": "172.20.4.2:5007", "weight": 1.0, "name": "variable"}
            ],
            "strategy": "ewma",
            "rebalance_interval_ms": 500
        },
        "encoder": {
            "bitrate_kbps": 4000,
            "min_kbps": 1000,
            "max_kbps": 8000,
            "step_kbps": 250
        }
    }
    
    import json
    with open(config_dir / "bonding-config.json", "w") as f:
        json.dump(config, f, indent=2)
    
    print("✓ Test configuration generated")

def generate_network_profiles():
    """Generate network profile definitions"""
    profiles_dir = Path("test-data/network-profiles")
    profiles_dir.mkdir(exist_ok=True, parents=True)
    
    profiles = {
        "good-4g": {
            "latency_ms": 50,
            "jitter_ms": 5,
            "loss_percent": 1.0,
            "bandwidth_mbps": 20,
            "description": "Good 4G connection"
        },
        "poor-4g": {
            "latency_ms": 150,
            "jitter_ms": 20,
            "loss_percent": 5.0,
            "bandwidth_mbps": 10,
            "description": "Poor 4G connection with high loss"
        },
        "5g": {
            "latency_ms": 20,
            "jitter_ms": 2,
            "loss_percent": 0.1,
            "bandwidth_mbps": 100,
            "description": "5G connection"
        },
        "variable": {
            "latency_ms": 75,
            "jitter_ms": 10,
            "loss_percent": 2.0,
            "bandwidth_mbps": 30,
            "description": "Variable quality connection",
            "variations": {
                "latency_range": [50, 200],
                "loss_range": [0.5, 10.0],
                "bandwidth_range": [5, 50]
            }
        }
    }
    
    import json
    with open(profiles_dir / "network-profiles.json", "w") as f:
        json.dump(profiles, f, indent=2)
    
    print("✓ Network profiles generated")

def main():
    print("Generating test content for RIST bonding tests...")
    
    generate_test_video()
    generate_test_config()
    generate_network_profiles()
    
    print("✓ Test content generation completed")

if __name__ == "__main__":
    main()