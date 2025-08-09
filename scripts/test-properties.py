#!/usr/bin/env python3

"""
Test element properties functionality
"""

import gi
gi.require_version('Gst', '1.0')
from gi.repository import Gst, GLib
import json

def test_dispatcher_properties():
    """Test ristdispatcher properties"""
    print("Testing ristdispatcher properties...")
    
    Gst.init(None)
    
    # Create dispatcher element
    dispatcher = Gst.ElementFactory.make('ristdispatcher', 'test-dispatcher')
    if not dispatcher:
        raise Exception("Failed to create ristdispatcher element")
    
    # Test initial property values
    tests = []
    
    # Test weights property
    try:
        initial_weights = dispatcher.get_property('weights')
        print(f"Initial weights: {initial_weights}")
        
        # Set new weights
        dispatcher.set_property('weights', '[2.0, 1.0, 1.5]')
        new_weights = dispatcher.get_property('weights')
        print(f"New weights: {new_weights}")
        tests.append(("weights", "PASS"))
    except Exception as e:
        print(f"Weights test failed: {e}")
        tests.append(("weights", "FAIL"))
    
    # Test strategy property
    try:
        initial_strategy = dispatcher.get_property('strategy')
        print(f"Initial strategy: {initial_strategy}")
        
        dispatcher.set_property('strategy', 'aimd')
        new_strategy = dispatcher.get_property('strategy')
        print(f"New strategy: {new_strategy}")
        tests.append(("strategy", "PASS"))
    except Exception as e:
        print(f"Strategy test failed: {e}")
        tests.append(("strategy", "FAIL"))
    
    # Test rebalance interval
    try:
        initial_interval = dispatcher.get_property('rebalance-interval-ms')
        print(f"Initial rebalance interval: {initial_interval}")
        
        dispatcher.set_property('rebalance-interval-ms', 1000)
        new_interval = dispatcher.get_property('rebalance-interval-ms')
        print(f"New rebalance interval: {new_interval}")
        tests.append(("rebalance-interval-ms", "PASS"))
    except Exception as e:
        print(f"Rebalance interval test failed: {e}")
        tests.append(("rebalance-interval-ms", "FAIL"))
    
    # Test auto-balance
    try:
        initial_auto = dispatcher.get_property('auto-balance')
        print(f"Initial auto-balance: {initial_auto}")
        
        dispatcher.set_property('auto-balance', False)
        new_auto = dispatcher.get_property('auto-balance')
        print(f"New auto-balance: {new_auto}")
        tests.append(("auto-balance", "PASS"))
    except Exception as e:
        print(f"Auto-balance test failed: {e}")
        tests.append(("auto-balance", "FAIL"))
    
    return tests

def test_dynbitrate_properties():
    """Test dynbitrate properties"""
    print("Testing dynbitrate properties...")
    
    # Create dynbitrate element
    dynbitrate = Gst.ElementFactory.make('dynbitrate', 'test-dynbitrate')
    if not dynbitrate:
        raise Exception("Failed to create dynbitrate element")
    
    tests = []
    
    # Test bitrate range properties
    try:
        initial_min = dynbitrate.get_property('min-kbps')
        print(f"Initial min-kbps: {initial_min}")
        
        dynbitrate.set_property('min-kbps', 1000)
        new_min = dynbitrate.get_property('min-kbps')
        print(f"New min-kbps: {new_min}")
        tests.append(("min-kbps", "PASS"))
    except Exception as e:
        print(f"Min-kbps test failed: {e}")
        tests.append(("min-kbps", "FAIL"))
    
    try:
        initial_max = dynbitrate.get_property('max-kbps')
        print(f"Initial max-kbps: {initial_max}")
        
        dynbitrate.set_property('max-kbps', 8000)
        new_max = dynbitrate.get_property('max-kbps')
        print(f"New max-kbps: {new_max}")
        tests.append(("max-kbps", "PASS"))
    except Exception as e:
        print(f"Max-kbps test failed: {e}")
        tests.append(("max-kbps", "FAIL"))
    
    try:
        initial_step = dynbitrate.get_property('step-kbps')
        print(f"Initial step-kbps: {initial_step}")
        
        dynbitrate.set_property('step-kbps', 500)
        new_step = dynbitrate.get_property('step-kbps')
        print(f"New step-kbps: {new_step}")
        tests.append(("step-kbps", "PASS"))
    except Exception as e:
        print(f"Step-kbps test failed: {e}")
        tests.append(("step-kbps", "FAIL"))
    
    try:
        initial_target = dynbitrate.get_property('target-loss-pct')
        print(f"Initial target-loss-pct: {initial_target}")
        
        dynbitrate.set_property('target-loss-pct', 1.5)
        new_target = dynbitrate.get_property('target-loss-pct')
        print(f"New target-loss-pct: {new_target}")
        tests.append(("target-loss-pct", "PASS"))
    except Exception as e:
        print(f"Target-loss-pct test failed: {e}")
        tests.append(("target-loss-pct", "FAIL"))
    
    return tests

def main():
    """Run all property tests"""
    print("Running property tests...")
    
    all_tests = []
    
    try:
        dispatcher_tests = test_dispatcher_properties()
        all_tests.extend([("dispatcher", test) for test in dispatcher_tests])
    except Exception as e:
        print(f"Dispatcher tests failed: {e}")
        all_tests.append(("dispatcher", ("overall", "FAIL")))
    
    try:
        dynbitrate_tests = test_dynbitrate_properties()
        all_tests.extend([("dynbitrate", test) for test in dynbitrate_tests])
    except Exception as e:
        print(f"Dynbitrate tests failed: {e}")
        all_tests.append(("dynbitrate", ("overall", "FAIL")))
    
    # Summary
    print("\nProperty Test Results:")
    passed = 0
    failed = 0
    
    for element, (prop, result) in all_tests:
        print(f"{element}.{prop}: {result}")
        if result == "PASS":
            passed += 1
        else:
            failed += 1
    
    print(f"\nSummary: {passed} passed, {failed} failed")
    
    if failed > 0:
        return 1
    return 0

if __name__ == "__main__":
    import sys
    sys.exit(main())