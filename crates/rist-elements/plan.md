RIST Bonding Performance Test Analysis
1. Incomplete Stream Reconstruction (Bonding vs. Redundancy)
Issue: The current design splits a single MPEG-TS stream across multiple “bonded” links but never
reassembles the pieces for decoding. In the performance test, the first link’s output is recorded directly to a
file – meaning that file only contains a fraction of the packets (e.g. ~25% if weights are equal). This
inevitably produces a corrupt or unplayable video since most data is missing. Earlier commits without
rebalancing may have appeared “somewhat working” likely because one link carried most of the data or all
links carried duplicate data, so a single-link capture was closer to a complete stream. In true load-balancing
mode, no single path has the full stream, so capturing one path yields an incomplete TS.
Implication: A proper bonding implementation must merge all link outputs before decoding or writing to
file . The test as written doesn’t do that – it effectively treats bonding like failover or redundancy, not
like a combined channel. This is a fundamental design gap: without an aggregator (or RIST’s own bonding
mode), the video will always be “corrupt” when split across links.
Suggestion: Simulate a real receiver. For example, send each ristdispatcher output through a
separate UDP/RIST sink in distinct network namespaces, then have a single ristsrc (or RTP jitterbuffer +
depay) combine the flows into one stream for the decoder/recorder. In RIST terms, you’d use the same
stream ID across multiple links and let the receiver handle reordering . Currently, the test only uses
counter_sink elements (which just count buffers) and then taps one link for recording, which isn’t
representative of real bonded streaming.
2. Testing Methodology – Lack of True Network Simulation
Issue: The “performance evaluation” test doesn’t actually introduce network differences like latency, loss, or
bandwidth constraints. It constructs a single GStreamer pipeline with a videotestsrc → x265enc
source and splits to counters . The code defines ConnectionProfile with loss and bandwidth
parameters , but these are never applied to any network interface or GStreamer element – they’re just
printed to the console . So, all four “links” are effectively identical and perfect (no added delay or
loss). Consequently, features like automatic rebalancing and adaptive bitrate never get meaningfully
triggered. The dispatcher has auto-balance=true and a 1000ms rebalance interval, but with no
variation in link stats (since no real RIST or TC simulation is feeding it), the weights likely remain static.
Similarly, dynbitrate isn’t reacting to any network changes (more on that below).
Implication: The test isn’t truly evaluating performance under varying conditions – it’s just measuring that
the pipeline can run for 30s and then dumping one link’s output. This might hide or misrepresent issues.
For example, any logic in the dispatcher for handling high RTT or loss isn’t being tested here, and
concurrency issues or race conditions might only show up when real stats come in asynchronously.
1 2
3 4
4
5
6
7
8
1
Suggestion: Incorporate the network-sim crate or Linux Traffic Control in the test. For example, set up
veth pairs and apply the ConnectionProfile parameters via tc qdisc (the project has utilities for
NetworkParams::good/typical/poor ). You could run separate sender/receiver pipelines in different
network namespaces to simulate real cellular links. This way, the ristdispatcher would receive distinct
feedback (RTT, NACKs) per link, exercising the rebalancing logic. Without this, the “performance” test isn’t
validating actual performance – it’s only checking pipeline assembly.
3. RistDispatcher Packet Handling and Ordering
Issue: The custom ristdispatcher element distributes RTP packets in a Weighted Round Robin fashion
but doesn’t ensure they arrive in order to the decoder. If one link has more latency than another, out-of-
order RTP sequence numbers will occur at the receiver. The current code doesn’t show any reordering
mechanism at the receive side. In fact, since the test doesn’t use a real ristsrc for bonding (just
counters and a single jitterbuffer on one path ), the out-of-order problem is masked. In a real
scenario, RIST would consider missing sequence numbers as packet loss. Each ristsink / ristsrc pair
is unaware it’s getting only a subset of packets, so it will likely spam NACKs for packets that went over other
links, wasting bandwidth and possibly causing stalls or timeouts.
Implication: This can manifest as video corruption or freezes in a real bonded setup. The “corruption” you
observed is likely due not only to missing data, but also timing issues. MPEG-TS over RTP expects in-order
delivery per stream. Without a coordinated reassembly, the decoder might get bursts of packets out-of-
sync, leading to artifacting or decode errors. Earlier (pre-rebalance) tests might have “worked” if essentially
one link bore most traffic (thus near-sequential), whereas dynamic rebalancing could increase reordering
frequency as weights shift.
Suggestion: Use RIST’s bonding capabilities if available (the RIST Spec Advanced Profile supports bonding
with sequence coordination). If not, consider implementing an aggregator element or have the ristsrc
listen on multiple sockets and merge the streams by sequence number. At minimum, a single
rtpjitterbuffer receiving all packets (from all links) is needed so it can sort out-of-order packets
before depayloading. The current approach of treating each link separately up to the application layer is
fundamentally flawed for bonding .
4. Dynamic Bitrate ( dynbitrate ) Misconfiguration
Issue: In the test pipeline, the dynbitrate element is inserted but not actually configured to do
anything. You create it and link it, but you never set its encoder or rist properties . Internally,
dynbitrate expects references to the encoder element (to adjust its bitrate property) and to one of the
RIST sink elements (to read stats) . Since these aren’t provided, the dynbitrate ’s periodic
tick() will bail out immediately, logging that no RIST or encoder is set . This means no adaptive
bitrate behavior is occurring at all during the test. The encoder (x265enc) stays at the initial 6000 kbps
constant bitrate, which the test confirms by asserting the final bitrate remains 5000 in a different scenario
using the stub .
Implication: Any logic in dynbitrate – PID controller, gentle ramp down, keyframe triggers on
downscale, etc. – isn’t being tested or used. Moreover, because you didn’t disable the dispatcher’s auto-
balance when using dynbitrate , you could have had two controllers interfering if dynbitrate were
9 10
4
11
12 13
14
15
2
active. The intended design (per docs) is to turn off auto-balance when dynbitrate is coordinating
weights . In the code, dynbitrate does this automatically if you set its dispatcher property, but
you didn’t. So in effect, you left auto-balance=true on the dispatcher and also inserted an (inactive)
dynbitrate in front. This mismatch suggests either a configuration oversight or a misunderstanding of how
the elements coordinate.
Suggestion: If you want adaptive bitrate in the test, properly set up dynbitrate . For example:
dynbitrate.set_property("encoder", &encoder_element);
dynbitrate.set_property("rist", &rist_sink_element);
dynbitrate.set_property("dispatcher", &dispatcher);
dynbitrate.set_property("target-loss-pct", 1.0f64);
And ensure auto-balance is false on the dispatcher when doing so (the setter will handle it as shown in
code). However, note that testing dynbitrate in the current single-process setup is limited since the RIST
stats are not real. It might be better to first get bonding working (with dispatcher auto-balance) and then
introduce dynbitrate once real stats are flowing. The bottom line is that the dynbitrate element as currently
used adds complexity without effect, and any supposed “adaptive” behavior isn’t happening. This could lead
to confusion when debugging, so either configure it properly or remove it for now.
5. Output Pipeline/Event Handling Issues
There are some lower-level details that could also be problematic:
Caps and Keyframes on Each Pad: The dispatcher caches stream-start, caps, and codec config
events to replay on new pads . If this logic is faulty, some pads might not get critical data. For
instance, if only one pad received the PAT/PMT or SPS/PPS at start and others didn’t, those other
streams would be undecodable on their own. In the recorded output (link 0), we might be seeing
corruption because that pad didn’t get a full set of initialization data. Earlier static tests might not
have revealed this if pads were added at pipeline start (all got caps) and no mid-stream pads were
requested. But with rebalancing, if you ever add/remove pads dynamically (or in a more complex
test), any mistake in event propagation will show up as corruption or dead streams on certain links.
Double-check that ristdispatcher sends the same caps and stream config down each src pad (it
appears to attempt this via cached events) and consider enabling the keyframe-duplicate
feature if you haven’t – that ensures every link gets keyframes periodically , which helps
recovery.
Hysteresis and Rebalancing Stability: In tests, you disabled min-hold-ms and set switch-
threshold low (via create_dispatcher_for_testing ) to allow instant weight changes.
Real networks are noisy; if your auto-balancing responds too quickly, weights will oscillate (flap) and
actually degrade performance. Ensure the production settings for min-hold-ms , rebalance-
interval , and hysteresis-window are tuned to avoid constant churning of weights .
The performance test as written doesn’t simulate this well, but in practice you’d want to see smooth
weight adjustments, not rapid swings. Instability here could cause momentary overload on a
previously “light” link (causing jitter/corruption) or starvation on a link that’s actually fine. If earlier
16
8
•
17
18 19
•
20
21 22
3
commits had no rebalancing, the system was effectively static and stable; introducing rebalancing
without proper damping could be a cause of new corruption (e.g. bursts of out-of-order packets
during weight transitions).
Test Duration vs. Buffering: You run the pipeline for 30s and then EOS + stop . If the
recorded file shows corruption predominantly at the end or start, it could be from startup/transient
effects or improper EOS handling. You do send EOS and wait for it, which is good . Just ensure
that each branch gets the EOS (the dispatcher should forward events to all source pads). The custom
elements need to pass along the EOS; your counter_sink element, for example, explicitly marks
got_eos when it sees one . If any branch didn’t handle EOS properly, the file sink might not
finalize the TS file correctly. Monitor the pipeline’s bus for any Error messages (you already do
partially). In the test output, setting GST_DEBUG=ristdispatcher:4,dynbitrate:4 might reveal
if something odd happens during rebalancing or teardown.
6. Next Steps to Fix and Validate
To summarize the fundamental fixes:
Implement true multi-link reception: Either use RIST Advanced Profile or simulate it by merging
flows. Without this, you’re not actually achieving bonding – you’re closer to doing per-packet load
splitting with no way to reassemble, which by definition yields a broken stream on any single output.
This is the number one issue to resolve.
Improve the test setup: Use the network-sim crate to impose different conditions on each link
and create a scenario where the benefits of bonding (e.g. surviving one bad link) can be observed.
The test should verify end-to-end video integrity (e.g. by hashing decoded frames or at least
counting TS packets received vs. sent across all links). Right now, counting buffers per link and
writing an incomplete file doesn’t tell you if bonding works – it only tells you that packets were
distributed.
Configure elements correctly: Only use dynbitrate when you have the RIST stats available and
an encoder to control. Otherwise, it can be left out to reduce variables. When you do use it, set it up
per design and disable the dispatcher’s internal rebalancing to avoid tug-of-war. Verify that the
dynbitrate actually changes the encoder’s bitrate in response to simulated loss by observing the
encoder’s bitrate property or the output file’s bitrate over time.
Monitor for corruption explicitly: Instead of visually guessing at corruption, include an automated
check. For example, you could pipeline the received TS into a decoder and a frame counter to ensure
frames increment monotonically. Or simpler: check if the recorded TS can be parsed without errors
(using something like ffprobe or GStreamer’s tsparse ). This way, your tests can catch
corruption regressions. Right now, the corruption was noticed manually; a good test would fail
automatically if the output is bad.
In summary, the concept of bonding is sound, but the current approach has a mismatch between what’s
implemented/tested and what’s needed for true multi-path streaming. Fixing the above foundational
issues – especially the stream recombination and realistic testing – will likely resolve the “corruption” and
• 23 24
24
25
•
•
•
•
4
give you a better platform to tune the load balancing and adaptive bitrate algorithms. Good luck, and happy
streaming!
Sources:
RIST bonding concept – splitting one RTP stream over multiple paths (requires recombination on
receive).
Project documentation on dispatcher & dynbitrate features (highlights need for coordinated
control).
Excerpts from the code showing current test setup and issues (distribution to counters and single-
path recording) .
performance_evaluation.rs
https://github.com/RephlexZero/rist-bonding/blob/4e82a1f1246086b88455b4702ef0ae6d82cfc8fd/crates/rist-elements/tests/
performance_evaluation.rs
README.md
https://github.com/RephlexZero/rist-bonding/blob/4e82a1f1246086b88455b4702ef0ae6d82cfc8fd/docs/plugins/README.md
RIST bonding and Load-Sharing Demystified
https://www.rist.tv/articles-and-deep-dives/2020/6/16/rist-bonding-and-load-sharing-demystified
dynbitrate.rs
https://github.com/RephlexZero/rist-bonding/blob/4e82a1f1246086b88455b4702ef0ae6d82cfc8fd/crates/rist-elements/src/
dynbitrate.rs
rist_integration.rs
https://github.com/RephlexZero/rist-bonding/blob/4e82a1f1246086b88455b4702ef0ae6d82cfc8fd/crates/rist-elements/tests/
scenarios/rist_integration.rs
dispatcher.rs
https://github.com/RephlexZero/rist-bonding/blob/4e82a1f1246086b88455b4702ef0ae6d82cfc8fd/crates/rist-elements/src/
dispatcher.rs
testing.rs
https://github.com/RephlexZero/rist-bonding/blob/846ae5224ddaada47cbb38cb31d90edf3388de53/crates/rist-elements/src/
testing.rs
test_harness.rs
https://github.com/RephlexZero/rist-bonding/blob/4e82a1f1246086b88455b4702ef0ae6d82cfc8fd/crates/rist-elements/src/
test_harness.rs
• 4
• 18 26
•
1 14
1 2 5 6 7 8 9 10 11 23 24
3 18 19 21 22 26
4
12 13 14 16
15
17
20
25
5