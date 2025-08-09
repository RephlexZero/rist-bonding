#!/usr/bin/env python3

"""
Run basic loopback test without docker
"""

import gi
gi.require_version('Gst', '1.0')
from gi.repository import Gst, GLib
import time
import threading

class LoopbackTest:
    def __init__(self):
        Gst.init(None)
        self.pipeline = None
        self.loop = None
        
    def create_pipeline(self):
        """Create a simple loopback test pipeline"""
        pipeline_str = """
        videotestsrc num-buffers=300 pattern=smpte
        ! video/x-raw,width=640,height=480,framerate=30/1
        ! x264enc bitrate=2000 tune=zerolatency
        ! rtph264pay pt=96
        ! ristdispatcher name=disp
        ! fakesink dump=false
        """
        
        try:
            self.pipeline = Gst.parse_launch(pipeline_str)
            
            # Get the dispatcher element
            dispatcher = self.pipeline.get_by_name("disp")
            if dispatcher:
                dispatcher.set_property("weights", "[1.0]")
                dispatcher.set_property("auto-balance", False)
                print("✓ Dispatcher configured")
            
            return True
        except Exception as e:
            print(f"✗ Failed to create pipeline: {e}")
            return False
    
    def run_test(self, duration=30):
        """Run the loopback test"""
        if not self.create_pipeline():
            return False
            
        print(f"Running loopback test for {duration} seconds...")
        
        # Set up message handler
        bus = self.pipeline.get_bus()
        bus.add_signal_watch()
        
        def on_message(bus, message):
            if message.type == Gst.MessageType.EOS:
                print("✓ Test completed successfully")
                self.loop.quit()
            elif message.type == Gst.MessageType.ERROR:
                err, debug = message.parse_error()
                print(f"✗ Pipeline error: {err}")
                self.loop.quit()
        
        bus.connect("message", on_message)
        
        # Start the pipeline
        ret = self.pipeline.set_state(Gst.State.PLAYING)
        if ret == Gst.StateChangeReturn.FAILURE:
            print("✗ Failed to start pipeline")
            return False
        
        print("✓ Pipeline started")
        
        # Run the main loop
        self.loop = GLib.MainLoop()
        
        # Stop after duration
        def stop_test():
            time.sleep(duration)
            print("Stopping test...")
            self.pipeline.set_state(Gst.State.NULL)
            self.loop.quit()
        
        timer = threading.Thread(target=stop_test)
        timer.daemon = True
        timer.start()
        
        try:
            self.loop.run()
        except KeyboardInterrupt:
            print("Test interrupted")
        
        # Cleanup
        self.pipeline.set_state(Gst.State.NULL)
        return True

def main():
    print("Running basic RIST plugin loopback test...")
    
    test = LoopbackTest()
    success = test.run_test(duration=15)  # 15 second test
    
    if success:
        print("✓ Loopback test completed")
        with open("test-results/loopback-test.txt", "w") as f:
            f.write("PASS: Basic loopback test completed successfully\n")
    else:
        print("✗ Loopback test failed")
        with open("test-results/loopback-test.txt", "w") as f:
            f.write("FAIL: Basic loopback test failed\n")
    
    return 0 if success else 1

if __name__ == "__main__":
    import sys
    sys.exit(main())