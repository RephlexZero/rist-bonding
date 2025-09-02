test integration::backpressure_simulation::test_blocked_output_handling ... ristdispatcher registered successfully
dynbitrate registered successfully
counter_sink registered successfully
encoder_stub registered successfully
riststats_mock registered successfully
Testing blocked output handling with 5 scenarios
Scenario: [1.0, 0.0, 1.0] - Output 1 completely blocked
Scenario: [1.0, 1.0, 0.0] - Output 2 completely blocked
Scenario: [0.0, 1.0, 1.0] - Output 0 completely blocked
Scenario: [1.0, 0.1, 1.0] - Output 1 severely degraded
Scenario: [1.0, 1.0, 1.0] - All outputs recovered
Blocked output test: 0 switch events
No switching events - system may maintain stability under blocking
ok
test integration::backpressure_simulation::test_multiple_slow_outputs ... ristdispatcher registered successfully
dynbitrate registered successfully
counter_sink registered successfully
encoder_stub registered successfully
riststats_mock registered successfully
Testing multiple slow outputs with 8 degradation steps
Step 1: All outputs normal
Step 2: Output 0 slightly slow
Step 3: Outputs 0,1 slow
Step 4: Outputs 0,1,2 slow
Step 5: Multiple outputs very slow
Step 6: Severe degradation
Step 7: Partial recovery
Step 8: Full recovery
Multiple slow outputs: 0 adaptations observed
No adaptations recorded - may be expected behavior
ok

Some tests were also panicking