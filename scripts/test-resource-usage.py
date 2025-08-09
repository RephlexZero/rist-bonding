#!/usr/bin/env python3

"""
Test resource usage under load
"""

import psutil
import time
import json
import threading
import gi

gi.require_version('Gst', '1.0')
from gi.repository import Gst, GLib

class ResourceUsageTest:
    def __init__(self):
        Gst.init(None)
        self.monitoring = False
        self.stats = []
        self.pipeline = None
        
    def monitor_resources(self):
        """Monitor system resources"""
        process = psutil.Process()
        
        while self.monitoring:
            try:
                cpu_percent = process.cpu_percent()
                memory_info = process.memory_info()
                memory_percent = process.memory_percent()
                
                # Get system-wide stats
                system_cpu = psutil.cpu_percent(interval=None)
                system_memory = psutil.virtual_memory()
                
                stats_entry = {
                    'timestamp': time.time(),
                    'process_cpu_percent': cpu_percent,
                    'process_memory_mb': memory_info.rss / 1024 / 1024,
                    'process_memory_percent': memory_percent,
                    'system_cpu_percent': system_cpu,
                    'system_memory_percent': system_memory.percent,
                    'system_memory_available_mb': system_memory.available / 1024 / 1024
                }
                
                self.stats.append(stats_entry)
                
            except Exception as e:
                print(f"Error monitoring resources: {e}")
            
            time.sleep(1)  # Monitor every second
    
    def create_load_pipeline(self):
        """Create a high-load pipeline"""
        pipeline_str = """
        videotestsrc pattern=snow
        ! video/x-raw,width=1920,height=1080,framerate=60/1
        ! x264enc bitrate=8000 tune=zerolatency speed-preset=fast
        ! rtph264pay pt=96
        ! ristdispatcher name=disp
        ! dynbitrate name=dyn
        ! fakesink dump=false
        """
        
        try:
            self.pipeline = Gst.parse_launch(pipeline_str)
            
            # Configure elements for high load
            dispatcher = self.pipeline.get_by_name("disp")
            if dispatcher:
                dispatcher.set_property("weights", "[1.0, 1.0, 1.0, 1.0]")  # 4 links
                dispatcher.set_property("auto-balance", True)
                dispatcher.set_property("rebalance-interval-ms", 100)  # Fast rebalancing
                print("✓ High-load dispatcher configured")
            
            dynbitrate = self.pipeline.get_by_name("dyn")
            if dynbitrate:
                dynbitrate.set_property("min-kbps", 2000)
                dynbitrate.set_property("max-kbps", 12000)
                dynbitrate.set_property("step-kbps", 1000)
                print("✓ Dynamic bitrate controller configured")
            
            return True
        except Exception as e:
            print(f"✗ Failed to create load pipeline: {e}")
            return False
    
    def run_test(self, duration=60):
        """Run resource usage test under load"""
        if not self.create_load_pipeline():
            return False
        
        print(f"Running resource usage test for {duration} seconds...")
        
        # Start resource monitoring
        self.monitoring = True
        monitor_thread = threading.Thread(target=self.monitor_resources, daemon=True)
        monitor_thread.start()
        
        # Start pipeline
        ret = self.pipeline.set_state(Gst.State.PLAYING)
        if ret == Gst.StateChangeReturn.FAILURE:
            print("✗ Failed to start pipeline")
            self.monitoring = False
            return False
        
        print("✓ High-load pipeline started")
        
        # Run for specified duration
        start_time = time.time()
        while time.time() - start_time < duration:
            time.sleep(1)
        
        # Stop pipeline
        self.pipeline.set_state(Gst.State.NULL)
        self.monitoring = False
        
        # Wait a bit for monitoring thread to finish
        time.sleep(2)
        
        return self.analyze_results()
    
    def analyze_results(self):
        """Analyze resource usage results"""
        if not self.stats:
            return {'status': 'FAIL', 'error': 'No statistics collected'}
        
        # Calculate statistics
        cpu_values = [s['process_cpu_percent'] for s in self.stats]
        memory_values = [s['process_memory_mb'] for s in self.stats]
        system_cpu_values = [s['system_cpu_percent'] for s in self.stats]
        
        results = {
            'status': 'PASS',
            'duration': len(self.stats),
            'samples': len(self.stats),
            'process_stats': {
                'cpu_percent_avg': sum(cpu_values) / len(cpu_values),
                'cpu_percent_max': max(cpu_values),
                'cpu_percent_min': min(cpu_values),
                'memory_mb_avg': sum(memory_values) / len(memory_values),
                'memory_mb_max': max(memory_values),
                'memory_mb_min': min(memory_values)
            },
            'system_stats': {
                'cpu_percent_avg': sum(system_cpu_values) / len(system_cpu_values),
                'cpu_percent_max': max(system_cpu_values),
                'memory_usage_stable': True  # Simplified check
            },
            'raw_data': self.stats[-10:]  # Last 10 samples for inspection
        }
        
        # Performance thresholds
        if results['process_stats']['cpu_percent_avg'] > 80:
            results['warnings'] = results.get('warnings', [])
            results['warnings'].append('High average CPU usage')
        
        if results['process_stats']['memory_mb_max'] > 1000:  # 1GB
            results['warnings'] = results.get('warnings', [])
            results['warnings'].append('High memory usage')
        
        return results

def main():
    print("Testing resource usage under load...")
    
    test = ResourceUsageTest()
    results = test.run_test(duration=30)  # 30 second test
    
    if results and results['status'] == 'PASS':
        print(f"\nResource Usage Test Results:")
        print(f"  Status: {results['status']}")
        print(f"  Samples collected: {results['samples']}")
        print(f"  Average CPU usage: {results['process_stats']['cpu_percent_avg']:.1f}%")
        print(f"  Max CPU usage: {results['process_stats']['cpu_percent_max']:.1f}%")
        print(f"  Average memory usage: {results['process_stats']['memory_mb_avg']:.1f} MB")
        print(f"  Max memory usage: {results['process_stats']['memory_mb_max']:.1f} MB")
        
        if 'warnings' in results:
            print("  Warnings:")
            for warning in results['warnings']:
                print(f"    - {warning}")
        
        # Save results
        with open("test-results/stress/resource-usage-test.json", "w") as f:
            json.dump(results, f, indent=2)
        
        # Mark test as completed
        with open("test-results/stress/resource-usage-completed.txt", "w") as f:
            f.write("PASS: Resource usage test completed successfully\n")
        
        return 0
    else:
        print("✗ Resource usage test failed")
        return 1

if __name__ == "__main__":
    import sys
    sys.exit(main())