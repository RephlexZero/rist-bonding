#!/usr/bin/env python3

"""
Test link failure recovery simulation
"""

import gi
gi.require_version('Gst', '1.0')
from gi.repository import Gst, GLib
import threading
import time
import json

class LinkFailureTest:
    def __init__(self):
        Gst.init(None)
        self.pipeline = None
        self.results = []
        self.dispatcher = None
        
    def create_test_pipeline(self):
        """Create pipeline with simulated multi-link setup"""
        pipeline_str = """
        videotestsrc pattern=ball num-buffers=1800
        ! video/x-raw,width=640,height=480,framerate=30/1
        ! x264enc bitrate=3000 tune=zerolatency
        ! rtph264pay pt=96
        ! ristdispatcher name=disp
        ! fakesink dump=false
        """
        
        try:
            self.pipeline = Gst.parse_launch(pipeline_str)
            self.dispatcher = self.pipeline.get_by_name("disp")
            
            if self.dispatcher:
                # Simulate 4 links with different weights
                self.dispatcher.set_property("weights", "[2.0, 1.5, 1.0, 0.5]")
                self.dispatcher.set_property("auto-balance", True)
                self.dispatcher.set_property("strategy", "ewma")
                print("✓ Multi-link dispatcher configured")
            
            return True
        except Exception as e:
            print(f"✗ Failed to create pipeline: {e}")
            return False
    
    def simulate_link_failure(self, link_index, failure_duration):
        """Simulate link failure by modifying weights"""
        if not self.dispatcher:
            return
        
        print(f"Simulating failure on link {link_index} for {failure_duration}s...")
        
        # Get current weights
        try:
            current_weights_str = self.dispatcher.get_property("weights")
            current_weights = json.loads(current_weights_str)
            
            # Record original weight
            original_weight = current_weights[link_index] if link_index < len(current_weights) else 1.0
            
            # Set failed link weight to 0
            if link_index < len(current_weights):
                current_weights[link_index] = 0.0
            else:
                # Extend weights if necessary
                while len(current_weights) <= link_index:
                    current_weights.append(1.0)
                current_weights[link_index] = 0.0
            
            # Apply failure
            self.dispatcher.set_property("weights", json.dumps(current_weights))
            print(f"Link {link_index} failed (weight set to 0)")
            
            # Wait for failure duration
            time.sleep(failure_duration)
            
            # Restore link
            current_weights[link_index] = original_weight
            self.dispatcher.set_property("weights", json.dumps(current_weights))
            print(f"Link {link_index} restored (weight set to {original_weight})")
            
        except Exception as e:
            print(f"Error simulating link failure: {e}")
    
    def run_test(self, duration=60):
        """Run link failure recovery test"""
        if not self.create_test_pipeline():
            return False
        
        print(f"Running link failure test for {duration} seconds...")
        
        # Start pipeline
        ret = self.pipeline.set_state(Gst.State.PLAYING)
        if ret == Gst.StateChangeReturn.FAILURE:
            print("✗ Failed to start pipeline")
            return False
        
        print("✓ Pipeline started")
        
        # Schedule link failures
        failure_schedule = [
            (10, 0, 5),   # At 10s, fail link 0 for 5s
            (25, 1, 8),   # At 25s, fail link 1 for 8s
            (40, 2, 3),   # At 40s, fail link 2 for 3s
            (50, 0, 5),   # At 50s, fail link 0 again for 5s
        ]
        
        def run_failure_schedule():
            start_time = time.time()
            for delay, link_idx, fail_duration in failure_schedule:
                # Wait until it's time for this failure
                while time.time() - start_time < delay:
                    time.sleep(0.1)
                
                # Run failure simulation in background
                threading.Thread(
                    target=self.simulate_link_failure,
                    args=(link_idx, fail_duration),
                    daemon=True
                ).start()
        
        # Start failure schedule
        scheduler_thread = threading.Thread(target=run_failure_schedule, daemon=True)
        scheduler_thread.start()
        
        # Monitor pipeline for duration
        start_time = time.time()
        error_occurred = False
        
        bus = self.pipeline.get_bus()
        
        while time.time() - start_time < duration:
            message = bus.timed_pop(Gst.CLOCK_TIME_NONE)
            if message:
                if message.type == Gst.MessageType.ERROR:
                    err, debug = message.parse_error()
                    print(f"Pipeline error during test: {err}")
                    error_occurred = True
                    break
                elif message.type == Gst.MessageType.EOS:
                    print("Pipeline completed")
                    break
            time.sleep(0.1)
        
        # Stop pipeline
        self.pipeline.set_state(Gst.State.NULL)
        
        # Analyze results
        test_passed = not error_occurred
        
        results = {
            'test_type': 'link_failure_recovery',
            'duration': duration,
            'failures_simulated': len(failure_schedule),
            'pipeline_survived': test_passed,
            'status': 'PASS' if test_passed else 'FAIL',
            'failure_schedule': failure_schedule
        }
        
        return results

def main():
    print("Testing link failure recovery...")
    
    test = LinkFailureTest()
    results = test.run_test(duration=60)
    
    if results:
        print(f"\nLink Failure Test Results:")
        print(f"  Status: {results['status']}")
        print(f"  Pipeline survived: {results['pipeline_survived']}")
        print(f"  Failures simulated: {results['failures_simulated']}")
        
        # Save results
        with open("test-results/stress/link-failure-test.json", "w") as f:
            json.dump(results, f, indent=2)
        
        # Mark test as completed
        with open("test-results/stress/link-failure-completed.txt", "w") as f:
            if results['status'] == 'PASS':
                f.write("PASS: Link failure recovery test completed successfully\n")
            else:
                f.write("FAIL: Link failure recovery test failed\n")
        
        return 0 if results['status'] == 'PASS' else 1
    else:
        print("✗ Link failure test failed to run")
        return 1

if __name__ == "__main__":
    import sys
    sys.exit(main())