Test Coverage Audit for RIST Dispatcher and DynBitrate Elements
Current Test Coverage Overview

Dispatcher Element (Custom GStreamer “ristdispatcher”): The test suite covers a wide range of functionality for the dispatcher element. Key aspects currently tested include:

Basic Element Creation & Properties: Creating the dispatcher and setting/getting its GObject properties (e.g. weights JSON, rebalance interval, strategy mode, boolean flags). For example, tests verify that setting properties like weights and rebalance-interval-ms persists correctly and that read-only properties like current-weights reflect the expected values
GitHub
GitHub
. Configuration flags such as strategy (“aimd”, “ewma”, etc.) and caps-any are also set and retrieved to ensure they can be manipulated as intended
GitHub
.

Pad Management (Multi-output Pads): The ability to request and release dynamic source pads is exercised. Tests request multiple src_%u pads and ensure they are created with correct names (e.g. “src_0”, “src_1”), and that the element’s pad counts update accordingly
GitHub
GitHub
. Basic sink pad discovery is also verified
GitHub
. Additionally, dynamic pad addition and removal at runtime is tested: one test starts the pipeline with a single output, then adds a second output pad and sink while the pipeline is playing to ensure the dispatcher can handle pads appearing on the fly
GitHub
GitHub
. Another test removes a pad mid-pipeline (unlinking and freeing a src pad) and confirms the dispatcher continues forwarding traffic to the remaining pad without errors
GitHub
GitHub
.

Pipeline Integration & Data Flow: The dispatcher is put into GStreamer pipeline scenarios to verify end-to-end behavior. Tests link a test source through the dispatcher to multiple sink elements and run the pipeline, confirming that buffers flow to all active outputs. For instance, with preset weight ratios (e.g. 80/20), after running the pipeline the buffer counts at each output reflect the weight distribution (first output receiving significantly more buffers)
GitHub
GitHub
. An equal-weights scenario (no weights provided) is likewise tested to ensure roughly even split between outputs
GitHub
GitHub
. A corner case with a zero-weight output is covered: one output is assigned a 0 weight while others have nonzero weights; the test confirms that the zero-weight pad receives no data and the other pads’ counts reflect the intended proportional split
GitHub
GitHub
. Basic caps negotiation is also validated by feeding an RTP-format source into the dispatcher – after a short run, both the dispatcher’s sink and src pad report negotiated caps (e.g. application/x-rtp on each), confirming the element forwards caps properly
GitHub
GitHub
.

State Transitions & Concurrency: There is a dedicated test for pipeline state changes, which transitions a pipeline (with source → dispatcher → sink) through READY, PAUSED, PLAYING, then back to PAUSED and NULL to ensure no deadlocks or crashes during state changes
GitHub
GitHub
. Although no assertions check internal dispatcher state across transitions, the fact that it proceeds without error indicates the dispatcher handles state changes gracefully. Another test spawns multiple threads reading output counters’ properties concurrently while the pipeline runs, to detect any thread-safety issues. This test verifies that concurrent property access (e.g. reading pad stats) does not cause panics or inconsistent behavior
GitHub
GitHub
.

Error Handling: The suite simulates an error scenario by linking an incompatible source (raw audiotestsrc) into the dispatcher (which expects RTP packets), forcing a caps negotiation failure. The test expects a GStreamer error message on the bus and then confirms the pipeline can still be taken to NULL (stopped) cleanly
GitHub
GitHub
. This validates that the dispatcher fails gracefully on caps mismatch and does not prevent pipeline teardown. General error conditions (such as invalid property inputs) are not explicitly covered for the dispatcher, but the presence of this caps-negotiation test indicates a focus on robust failure handling in pipelines.

Auto-Rebalancing Logic: One scenario test exercises the dispatcher’s automatic rebalance feature using mocked RIST statistics. In this test, the dispatcher is configured with auto-balance=true and a strategy (EWMA) and given two output pads. A RistStatsMock element feeds in stats indicating one link with high loss/latency and another with good performance, then later “recovers” the poor link
GitHub
GitHub
. The pipeline is run in two phases around this stat change. The test ensures that initially the better link (session 0) carries more traffic, and after improving session 1’s stats, both outputs continue to receive traffic (i.e. load rebalancing occurs without starving either path)
GitHub
GitHub
. This confirms the dispatcher can react to changing network stats, though the verification is coarse (checking both counters increased or at least maintained their counts).

Dynamic Bitrate Element (“dynbitrate”): The dynbitrate controller element’s tests focus on its core adaptive bitrate logic and basic integration points:

Element Creation & Properties: Tests instantiate the dynbitrate element to ensure it registers correctly and its default property values fall in expected ranges (e.g. default min-kbps > 0, max-kbps > min-kbps, and target-loss-pct within a valid percent range)
GitHub
. Setting properties is verified as well – for example, adjusting min-kbps, max-kbps, and target-loss-pct to specific values and reading them back to confirm the setters work and basic validation (e.g. min-kbps and max-kbps accept new values and maintain their relationship)
GitHub
GitHub
. These ensure no obvious issues with property getters/setters for configuration parameters.

Bitrate Adaptation Logic: The test suite thoroughly exercises dynbitrate’s adaptive behavior using a fake encoder element (encoder_stub) and a mocked RIST stats source (riststats_mock). Several integration tests simulate different network conditions and verify the dynbitrate element’s response in adjusting the encoder’s bitrate:

Decrease on High Loss/Latency: Under conditions of high packet loss (e.g. ~9% retransmissions) and high RTT, the dynbitrate element is expected to lower the encoder bitrate. The test initializes the encoder at 5000 kbps, feeds stats indicating poor network (high loss/RTT), then waits enough time for dynbitrate’s periodic tick and rate-limit interval. It confirms the encoder’s bitrate property decreases by approximately one step (e.g. 500 kbps) but not below the configured minimum
GitHub
GitHub
.

Increase on Good Network: Conversely, with near-zero loss and low RTT, dynbitrate should raise the bitrate. A test starts at 5000 kbps with ideal stats (0% loss, low RTT) and checks that after the adjustment interval, the encoder’s bitrate increases by one step (capped at the max limit)
GitHub
GitHub
.

Deadband (No Change Zone): Dynbitrate has a deadband around the target loss percentage to avoid oscillation. A test feeds ~1.0% loss when the target-loss is 1.0% (within the neutral zone) and confirms that after the tick, the bitrate remains unchanged
GitHub
GitHub
.

Rate Limiting: The element enforces a minimum interval between successive bitrate changes. A test triggers one bitrate drop, then waits only a short time (less than the rate limit) before simulating another drop condition. It verifies that the second adjustment does not occur too early (bitrate stays the same in the interim), and only after the full rate-limit duration passes does a second drop happen
GitHub
GitHub
. This ensures the internal timing logic to prevent rapid oscillations is working.

Min/Max Bounds: Tests drive the bitrate to its extremes to verify clamping. By using an exaggerated step size, one scenario forces repeated decreases until the bitrate hits the configured minimum and checks it never goes below that floor
GitHub
GitHub
. It then simulates a recovery (zero loss) to allow increases and ensures the bitrate tops out at the max limit without exceeding it
GitHub
.

Pipeline Pass-through & Integration: A basic integration test places the dynbitrate element in a simple pipeline (source → encoder_stub → dynbitrate → fakesink) and transitions the pipeline to Playing for a short time
GitHub
GitHub
. While this particular test does not feed any RIST stats (meaning no bitrate changes occur), it serves to confirm that dynbitrate can be linked in a pipeline and pass data without introducing errors or deadlocks. Essentially, it checks that the presence of dynbitrate doesn’t break the data flow or initial encoder settings (the encoder remains at the starting bitrate of 5000 kbps in the brief run)
GitHub
. This complements the focused logic tests by ensuring the element behaves as a transparent pass-through in normal conditions.

Breadth and Depth of Coverage

Overall, the test coverage for both elements is broad, exercising many functional paths and some edge cases. For the dispatcher, the tests span from basic usage (properties, pad setup) to complex scenarios (dynamic pad changes, multi-output weight distributions, error injection, stat-based rebalancing). This indicates a deep validation of its functionality under various conditions. Notably, unusual scenarios like zero-weight outputs
GitHub
, adding/removing pads at runtime
GitHub
GitHub
, and handling caps mismatches
GitHub
 are included, demonstrating attention to edge-case robustness. The dispatcher’s integration with other elements is also covered (e.g. working with RTP caps, test sink elements, and stat feeders), suggesting confidence that it interoperates correctly within a GStreamer pipeline.

The dynbitrate element’s tests are focused and thorough in validating its core algorithm. The range of simulated network conditions (good vs bad vs neutral) and the verification of internal constraints (deadband, rate limit, min/max bounds) give strong coverage of its adaptive behavior
GitHub
GitHub
. The use of a controlled test harness (encoder stub and stats mock) allows the suite to probe edge conditions (like hitting exact target loss or extreme step changes) that ensure the logic is behaving as designed. Basic properties and pipeline integration are also tested, covering both configuration and runtime operation.

Depth of scenarios: Functional scenarios (normal operations) are well-covered for both elements, and many edge cases are explicitly tested (e.g., dispatcher’s zero-weight and pad removal; dynbitrate’s no-change zone and clamping logic). However, some complex behaviors are only partially tested. For instance, the dispatcher’s auto-balancing is demonstrated in one scenario with a single stats change
GitHub
GitHub
; the test checks that traffic was distributed to both outputs, but it doesn’t explicitly assert a shift in ratio or a specific decision threshold being honored. Similarly, properties like hysteresis (min-hold-ms) and warm-up intervals on the dispatcher are set in tests, but their effect on behavior (preventing rapid switching or delaying initial balancing) isn’t validated with a dynamic scenario – the tests simply ensure these properties accept values
GitHub
GitHub
. In dynbitrate, the core timing and adjustment logic is thoroughly validated, but some features (like forcing keyframes on downscale, multi-session stats handling, or changing settings at runtime) are not exercised.

In summary, the test suite provides a strong foundation and covers most typical and many atypical scenarios for both components. There is a clear emphasis on functional correctness and stability under edge conditions. The few gaps that remain are in more nuanced or indirect behaviors (e.g. dynamic reconfiguration during runtime, certain state/coordination aspects), which we highlight below.

Gaps and Untested Areas

Despite the extensive coverage, a few areas are untested or under-tested, especially in terms of nuanced runtime dynamics and certain failure modes:

Dispatcher Hysteresis & Warm-Up Behavior: The dispatcher element exposes min-hold-ms (minimum hold time before switching outputs) and switch-threshold (performance improvement required to switch) as well as a health-warmup-ms. While tests do set these properties to confirm they can be configured
GitHub
GitHub
, there are no tests simulating a scenario where these parameters come into play. In other words, there is no verification that the dispatcher indeed refrains from switching outputs too quickly or during the warm-up period. Without a test driving a situation where one path’s quality flaps or improves just enough to cross the threshold, the hysteresis logic remains unvalidated in practice.

Dynamic Weight or Strategy Changes at Runtime: The suite does not currently test what happens if the dispatcher’s distribution weights or strategy are changed while the pipeline is live. All weight distribution tests assign weights at creation time. There’s a basic check that updating the weights property reflects in current-weights
GitHub
, but this is done without the pipeline actively routing data. A potential gap is whether changing weights or switching the balancing strategy on-the-fly seamlessly updates the output distribution or if it causes any transient issues. State transitions combined with config changes (e.g., altering weights during PAUSED or PLAYING states) are untested.

Full Auto-Rebalance Dynamics: Only a single rebalance cycle is tested with auto-balance (stats-driven mode) in the dispatcher
GitHub
GitHub
. The test asserts that both outputs eventually carried traffic, but it doesn’t deeply examine how the traffic split changed in response to the stats update (it checks only that neither output was starved). Edge cases in auto-balance, such as oscillating network conditions or multiple sequential stat updates, aren’t covered. Also, different strategy algorithms (e.g., “aimd” vs “ewma”) are not compared in behavior – the tests set them but do not confirm their distinct effects.

Error and Misuse Scenarios (Dispatcher): Aside from the caps negotiation error, other potential error conditions are not explicitly tested. For example, what if an invalid weights JSON is provided, or an extreme number of output pads is requested? Memory or performance issues with many pads, or providing mismatched weight array lengths, are not exercised in tests. The current tests also don’t simulate a downstream element failure (e.g., one sink element throwing an error or blocking); thus, the dispatcher’s robustness in the face of one output stalling is not directly verified.

DynBitrate Coordination Properties: The dynbitrate element has an unused dispatcher property and a downscale-keyunit option (to request a keyframe when scaling down bitrate)
GitHub
. These features are not covered by any tests. There is no test to ensure that when downscale-keyunit=true and a bitrate drop occurs, a keyframe (force-key-unit event) is triggered upstream. Similarly, if the dispatcher property is intended for coordinating with the RIST dispatcher (for example, pausing the dispatcher or aligning decision timing), no test currently sets or exercises this interaction. This coordination logic, if implemented, remains unvalidated.

DynBitrate with Missing or Multiple Stats Inputs: All dynbitrate behavior tests attach a stats source (the riststats_mock) before running. The case where the dynbitrate element is activated without any RIST stats element (or if the stats element produces no data) isn’t explicitly tested – in the integration pipeline test, no stats were connected, but the test did not check dynbitrate’s internal behavior under those conditions
GitHub
. It’s likely the element simply makes no adjustments without stats, but a test could confirm no spurious behavior or errors occur. Additionally, dynbitrate is tested with only one RIST session’s stats at a time (the mock is set to 1 session in all adaptation tests). If the design supports aggregating multiple session stats (e.g., combined loss across two links), there’s no test covering how the element behaves with multiple sessions reporting concurrently.

Pipeline State and Restart for DynBitrate: There isn’t a targeted test for how dynbitrate behaves across pipeline pausing or multiple start/stop cycles. For instance, if the pipeline is paused and then resumed, does dynbitrate correctly resume its timer and avoid carrying over stale state incorrectly? The clean_shutdown helper is used to ensure the timer is cleaned up after tests
GitHub
GitHub
, but we don’t see a test where the pipeline is paused mid-run and then resumed to see if bitrate adjustments continue smoothly. This is a minor gap, but relevant for scenarios where streaming may be interrupted or restarted.

Recommendations for Improved Testing

To achieve more comprehensive validation of these components, we suggest adding the following test cases and strategies, each addressing the gaps identified:

Hysteresis and Warm-Up Enforcement Test: Create a scenario with two dispatcher outputs where one path’s reported quality fluctuates. For example, start with both paths equal, then introduce a slight improvement on one path that does not meet the switch-threshold and occurs within the min-hold-ms period. Verify that the dispatcher does not immediately shift all traffic to the slightly better path. Then, after the warm-up interval and beyond the hold time, dramatically improve one path’s stats to exceed the threshold and check that the dispatcher finally rebalances in favor of that path. This test would confirm that min-hold-ms and switch-threshold truly delay or prevent flapping, rather than just being settable properties
GitHub
GitHub
.

Runtime Weight Update Handling: Add a test where the pipeline is running with the dispatcher, and mid-stream a change is made to the weights property (or the auto-balance mode is toggled). For instance, start with equal weights distributing evenly, then during playback call dispatcher.set_property("weights", "[1.0, 0.0]") to simulate pulling all traffic to one output. Use the counters to assert that after a short interval, the distribution reflects the new weights (one counter stops increasing). Similarly, test switching the strategy (e.g., from “swrr” to “aimd”) on the fly and ensure the element doesn’t error and begins using the new algorithm (this might be observed indirectly by checking property values or internal state if exposed). This would cover dynamic reconfiguration, ensuring the element adapts without requiring a pipeline restart.

Extended Auto-Rebalancing Scenario: Enhance the stats-driven test to cover multiple rebalance cycles. For example, alternate the network condition several times (degrade session 1, recover, degrade again) while keeping the pipeline running for an extended period. Track the current-weights property or the counters over time to assert that weights shift appropriately with each change (e.g., when session 1 degrades, its corresponding pad weight decreases as traffic shifts to the better link, and vice versa upon recovery). This can also include a check that the dispatcher does not over-react (thanks to hysteresis) if fluctuations are minor. Such a test would closely examine the dynamic behavior of the “auto-balance” feature beyond a single step change. If possible, verifying internal weight updates (perhaps via the current-weights string or debug logs) would make the assertions stronger.

Slow Sink / Backpressure Simulation: Introduce a test where one of the dispatcher’s outputs is deliberately made slow or prone to buffering, to observe how the dispatcher handles it. This could be done by linking one src pad to a queue element with a tiny max-size (to simulate blockage) or using a custom sink that introduces delays. The expectation is that the dispatcher, if well-designed, should not be permanently stalled by one slow consumer (GStreamer’s pad allocator should queue or drop frames as needed). The test can run the pipeline for a while and assert that at least the fast output continues to get data even if the slow one lags or that an error is posted if the design expects to signal overload. This would test the dispatcher’s resilience to uneven consumer speeds – an important real-world edge case for bonding multiple links.

Downscale Keyframe Trigger (DynBitrate): Add a unit test for the downscale-keyunit feature. Configure dynbitrate with downscale-keyunit=true, attach the encoder stub (which could be extended to record if it receives a force keyframe event on bitrate drop). Simulate a large drop in bitrate (e.g. from 8000 kbps to 4000 kbps, exceeding a 1.5× downscale ratio)
GitHub
. Then check that a keyframe request event was sent to the encoder (this might require a pad probe on the encoder’s sink pad to catch upstream GstForceKeyUnit events, or the encoder stub could log such events). Ensuring this behavior via test will guarantee that the keyframe logic works, preventing quality issues when bitrate is reduced.

DynBitrate Multi-Session and No-Stats Cases: Develop tests to cover how dynbitrate handles varying stats inputs:

No Stats: Run the dynbitrate pipeline without setting a RIST stats element at all (or explicitly set its rist property to NULL) and ensure that it doesn’t attempt adjustments (bitrate stays at default) and, importantly, that no errors or panic occur due to missing stats. This can simply assert that after a few tick intervals, the encoder’s bitrate is unchanged and the pipeline is still functioning.

Multiple Sessions: Use the RistStatsMock to simulate two or more sessions simultaneously (e.g., mock.set_sessions(2) and then call tick with arrays for two sessions). Before running, set dynbitrate’s target loss and other properties appropriately. Then, after the dynbitrate tick, check how it decides the adjustment. This requires knowing the intended logic (whether it reacts to the worst session or an aggregate). The test might assert that the bitrate drops if any session is well above target loss (if design is conservative) or only increases when all sessions are below target. If the logic isn’t documented, this test at least ensures multi-session input doesn’t crash and yields a deterministic outcome. It would extend coverage to scenarios akin to bonding (multiple links stats feeding one controller).

Lifecycle and State Change Tests for DynBitrate: Although dynbitrate is mostly a passive controller, adding a test that explicitly pauses and resumes the pipeline can be useful. For instance, start a dynbitrate pipeline, let it make one adjustment, then pause for a while (longer than its tick interval) and resume. Verify that it doesn’t make an extra adjustment immediately upon resuming (i.e., the rate-limiter still honors the last change time across a pause) and that it continues ticking normally after. Also, test disposing the dynbitrate element (remove it from pipeline or drop the pipeline without clean_shutdown) to see if the internal timeout source is cleared without memory leaks or warnings. This would ensure robustness in start/stop scenarios and proper cleanup.

Integration with Actual RIST Elements (if available): Finally, if the environment allows, enable the ignored test that uses real ristsrc/ristsink elements
GitHub
GitHub
. Using real network components in a test environment might be challenging, but even a simplified end-to-end test (looping a ristsink to ristsrc on localhost) could catch integration issues. This test should include the dispatcher and dynbitrate together in a realistic pipeline (e.g., source → dispatcher → ristsink, and ristsrc → dynbitrate → sink) to see how these elements interoperate in tandem. The expectation is that the dispatcher distributes packets into multiple ristsinks, and dynbitrate reads from those (or one aggregated ristsrc) adjusting an encoder. While complex, this scenario would closely mimic real deployment and could reveal any missed race conditions or negotiation issues when all pieces are combined. Even without full automation, running such a pipeline manually (or as an integration test requiring specific hardware/network) can be a valuable validation step.

Each of the above recommendations targets a specific gap in the current tests. Implementing these tests would provide actionable feedback to developers by either confirming the components handle these situations or revealing bugs to be fixed. By expanding coverage to these edge scenarios – especially around state changes, dynamic reconfiguration, and cross-component interactions – the confidence in the dispatcher and dynbitrate elements’ correctness and robustness will significantly increase. The goal is to ensure that every important behavior (from rate adjustment timing to pad lifecycle and error recovery) is validated by at least one test case, preventing regressions as the code evolves.