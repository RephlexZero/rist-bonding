#!/usr/bin/env python3

"""
Test multiple simultaneous streams
"""

import gi
gi.require_version('Gst', '1.0')
from gi.repository import Gst, GLib
import threading
import time
import json

class MultiStreamTest:
    def __init__(self, num_streams=3):
        Gst.init(None)
        self.num_streams = num_streams
        self.pipelines = []
        self.results = []
        
    def create_stream_pipeline(self, stream_id, bitrate):
        """Create a single stream pipeline"""
        pipeline_str = f"""
        videotestsrc pattern={stream_id % 20} num-buffers=600
        ! video/x-raw,width=320,height=240,framerate=30/1
        ! x264enc bitrate={bitrate} tune=zerolatency
        ! rtph264pay pt=96
        ! ristdispatcher name=disp{stream_id}
        ! fakesink dump=false
        """
        
        try:
            pipeline = Gst.parse_launch(pipeline_str)
            
            # Configure dispatcher
            dispatcher = pipeline.get_by_name(f"disp{stream_id}")
            if dispatcher:
                dispatcher.set_property("weights", "[1.0, 0.5]")  # Simulate 2 links
                dispatcher.set_property("auto-balance", False)
            
            return pipeline
        except Exception as e:
            print(f"Failed to create pipeline {stream_id}: {e}")
            return None
    
    def run_stream(self, stream_id, bitrate, duration):
        """Run a single stream"""
        pipeline = self.create_stream_pipeline(stream_id, bitrate)
        if not pipeline:
            self.results.append({
                'stream_id': stream_id,
                'status': 'FAIL',
                'error': 'Pipeline creation failed'
            })
            return
        
        print(f"Starting stream {stream_id} with {bitrate}kbps...")
        
        start_time = time.time()
        
        # Start pipeline
        ret = pipeline.set_state(Gst.State.PLAYING)
        if ret == Gst.StateChangeReturn.FAILURE:
            self.results.append({
                'stream_id': stream_id,
                'status': 'FAIL',
                'error': 'Failed to start pipeline'
            })
            return
        
        # Wait for duration
        time.sleep(duration)
        
        # Stop pipeline
        pipeline.set_state(Gst.State.NULL)
        
        end_time = time.time()
        
        self.results.append({
            'stream_id': stream_id,
            'status': 'PASS',
            'bitrate_kbps': bitrate,
            'duration': end_time - start_time,
            'start_time': start_time,
            'end_time': end_time
        })
        
        print(f"Stream {stream_id} completed")
    
    def run_test(self, duration=30):
        """Run multiple streams simultaneously"""
        print(f"Running {self.num_streams} simultaneous streams for {duration} seconds...")
        
        threads = []
        bitrates = [1000, 2000, 3000, 4000, 5000]  # Different bitrates
        
        for i in range(self.num_streams):
            bitrate = bitrates[i % len(bitrates)]
            thread = threading.Thread(
                target=self.run_stream,
                args=(i, bitrate, duration)
            )
            threads.append(thread)
        
        # Start all streams
        start_time = time.time()
        for thread in threads:
            thread.start()
        
        # Wait for all to complete
        for thread in threads:
            thread.join()
        
        end_time = time.time()
        
        # Analyze results
        passed = sum(1 for r in self.results if r['status'] == 'PASS')
        failed = sum(1 for r in self.results if r['status'] == 'FAIL')
        
        summary = {
            'total_streams': self.num_streams,
            'passed': passed,
            'failed': failed,
            'success_rate': passed / self.num_streams * 100,
            'total_duration': end_time - start_time,
            'results': self.results
        }
        
        return summary

def main():
    print("Testing multiple simultaneous streams...")
    
    # Test with 3 streams
    test = MultiStreamTest(num_streams=3)
    results = test.run_test(duration=20)
    
    print(f"\nMultiple Streams Test Results:")
    print(f"  Total streams: {results['total_streams']}")
    print(f"  Passed: {results['passed']}")
    print(f"  Failed: {results['failed']}")
    print(f"  Success rate: {results['success_rate']:.1f}%")
    
    # Save results
    with open("test-results/stress/multiple-streams-test.json", "w") as f:
        json.dump(results, f, indent=2)
    
    # Mark stress test as completed
    with open("test-results/stress/multiple-streams-completed.txt", "w") as f:
        if results['success_rate'] >= 66.7:  # At least 2/3 success
            f.write("PASS: Multiple streams test completed successfully\n")
        else:
            f.write("FAIL: Multiple streams test had too many failures\n")
    
    return 0 if results['success_rate'] >= 66.7 else 1

if __name__ == "__main__":
    import sys
    sys.exit(main())