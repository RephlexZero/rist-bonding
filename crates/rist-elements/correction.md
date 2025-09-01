Understanding RIST Bonding in GStreamer and
Best Practices
RIST Bonding vs. Seamless Redundancy
The RIST protocol supports multi-link operation in two ways :
Seamless Switching (Full Redundancy) – Every packet is duplicated on all links (akin to SMPTE
2022-7). The receiver’s jitter buffer reorders packets and discards duplicates, yielding hitless failover
if a link drops . Only packets lost on all links trigger retransmission requests, reducing NACK
overhead. This mode requires double (or more) bandwidth but provides zero-interruption switching.
Bonding (Load Sharing) – Traffic is split across multiple links (no intentional duplicates) to
aggregate bandwidth of all paths . The receiver collects packets from all links and reassembles
them in order, increasing total throughput. If one path slows or loses packets, the others carry the
load. This mode benefits from adaptive load balancing (e.g. shifting traffic off a congested link) .
It can also keep a backup link mostly idle until needed (for example, a cellular link held in reserve) to
save cost . Bonding is powerful but demands careful reordering and loss recovery at the receiver,
since out-of-order arrival is common.
GStreamer’s RIST Elements and Bonding Implementation
GStreamer (since ~1.18+ and improved by 1.24) provides ristsink (sender) and ristsrc (receiver)
elements that implement the RIST TR-06-1 spec . Internally, each is a GstBin built around
GStreamer’s RTP stack ( rtpbin with RTP/RTCP) . Key features like NACK-based retransmission,
reordering buffers, and RTCP are handled inside.
Bonding support is built into these elements: you can configure multiple network addresses on a single
ristsink/ristsrc to enable multi-path streaming. On the sender side, the ristsink element has a
bonding-addresses property that accepts a list of target addresses (each with IP:port). When this is set,
it overrides the single “address” and spawns multiple sub-sessions – one for each link . Similarly,
ristsrc can listen on multiple endpoints for the same stream . Each link gets its own RTP session
internally, but all sessions feed one common transport stream. For example:
Sender pipeline example:
... ! ristsink bonding-addresses="192.168.1.32:8000,127.0.0.1:8000" bonding-
method="broadcast" – This sends the stream to two addresses. If bonding-
method="broadcast" , a copy of every RTP packet goes to both links (full redundancy) . If
set to "round-robin" , packets are evenly distributed across links (load sharing) .
Receiver pipeline example:
gst-launch-1.0 ristsrc bonding-
1 2
•
1
•
2
2
3
4 5
6
7
8
•
9 10
11
•
1
addresses="192.168.1.32:8000,127.0.0.1:8000" ! ... – A single ristsrc element will
accept packets from both network endpoints and output one unified RTP stream . Internally, all
incoming links share one jitter buffer and reordering logic, so the receiver can assemble the
original sequence and avoid duplicate NACKs for packets that arrived on either link . In other
words, you do not need multiple separate ristsrc elements – one ristsrc with bonding
enabled will handle recombination and de-duplication.
How GStreamer implements bonding: By default, GStreamer’s RIST plugin uses simple strategies for
multi-link: for seamless mode, it internally uses a tee element to duplicate packets over each link, and for
load-sharing, it uses a round-robin distributor element . These are built-in when you set bonding-
method="broadcast" or "round-robin" . On the receive side, the RIST plugin ensures that the multiple
UDP sub-sources feed into one rtpjitterbuffer so that ordering and loss recovery are coordinated .
This design (multiple sessions sharing a single retransmission buffer and jitterbuffer) is crucial – it
means the sender tracks packet history across all links, and the receiver doesn’t request a retransmit if a
packet came in on any link . This prevents inefficient duplicate retransmissions and keeps latency lower.
Custom Dispatchers for Advanced Bonding Strategies
While the built-in bonding-method options are fixed (all-or-nothing broadcast or uniform round-robin),
GStreamer also allows you to plug in a custom dispatcher element for more sophisticated bonding logic
. The ristsink element provides a dispatcher property that can accept a user-provided element
to determine how to distribute RTP packets across links. This is where a custom ristdispatcher (like the
one in the user’s Rust plugin) comes in.
A custom dispatcher is essentially an element with one sink pad (receiving the single RTP stream from the
encoder) and multiple source pads (each feeding one link’s ristsink sub-session). GStreamer will use it
inside the ristsink bin instead of the default round-robin. For example, a ristdispatcher element
can implement Smooth Weighted Round Robin (SWRR) or other algorithms to dynamically adjust how
much traffic goes to each path . The dispatcher’s job is to decide “next packet out goes to link X”
based on current link quality metrics.
Single sender & receiver vs. multiple: With a custom dispatcher approach, you still only need one
ristsink element (which internally handles all links) and one ristsrc on the far end. The custom
dispatcher simply orchestrates packet distribution to the multiple outgoing sub-connections. So a single
RIST sender/receiver pair is sufficient to aggregate multiple links – you do not instantiate multiple separate
RIST sinks or sources. In fact, using one logical sender/receiver is recommended, because it ensures unified
sequence numbering and reordering. (If you naively used multiple independent RIST flows without an
aggregator, you’d have to merge them manually and could end up with out-of-sync sequence numbers or
redundant retransmission requests.) GStreamer’s bonded ristsrc already “handles reordering and
recombination” in bonded mode internally, so you don’t need multiple ristsrc elements for each link
.
Implementing the dispatcher: In GStreamer 1.24+, you can set the dispatcher property of ristsink
to a custom element (programmatically). The custom element must have the appropriate pad topology (one
sink, N sources) so that ristsink can request pads from it. For instance, the Rust-based
ristdispatcher described in the user’s project is designed for this role. It exposes requestable
8
12
11
13
14
11
15 16
17 8
2
src_%u pads and forwards all incoming data to one of those pads according to its scheduling algorithm
. You would create the dispatcher element, then assign it to ristsink before playing the pipeline.
Once attached, ristsink will use it to fan out packets, instead of using its internal round-robin or tee
. (Note: In gst-launch syntax, using a custom dispatcher inside ristsink may not be
straightforward; it’s typically done via the GStreamer API by linking the elements manually or by child-proxy
properties.)
Dynamic Load Balancing via RIST Statistics
One key advantage of bonding is the ability to adaptively load-balance based on link conditions. The RIST
protocol provides continuous feedback (RTCP statistics, NACK counts, round-trip time (RTT) measurements,
etc.) which can inform decisions. Best practice is to enable and monitor RIST link statistics on both
sender and receiver:
On the Receiver: GStreamer’s ristsrc exposes a read-only stats structure property (and
possibly posts it on the bus at intervals) containing metrics like packets received, lost, reordered,
etc., per link . Ensuring stats-update-interval is set (e.g. 500ms or 1000ms) will give
timely updates . These stats can help you observe if one path is dropping packets or has higher
latency.
On the Sender: Each ristsink (or each sub-session within it) tracks how many NACK (retransmit)
requests are coming from that receiver, the RTT for each link (via RTCP), and throughput. In
GStreamer’s plugin, if bonding-addresses are used, the built-in round-robin doesn’t auto-adjust
by itself – it will just cycle evenly. Custom dispatchers, however, can use the stats to adjust weights.
For instance, the ristdispatcher element monitors per-link RTT, packet loss rate, and
retransmission rate via the RIST element’s feedback messages . Links with low RTT and low loss
can be given higher weight, whereas a link showing high loss or latency would get less traffic
. This real-time adjustment yields a form of adaptive load sharing: the traffic allocation changes
in response to network performance.
How stats are accessed: The dispatcher can retrieve stats from ristsink through GStreamer messages or
properties. In the Rust plugin example, the ristdispatcher listens for “element messages” (or sticky
events) on its pads carrying RIST stats (e.g. updated RTT, loss) emitted by the ristsink sessions .
Those are stored in the dispatcher's state and used to recompute weights periodically (e.g. every
rebalance-interval ms) . GStreamer’s design ensures the feedback from each link is fed
upstream so a smart element can react.
Failover detection: In bonding mode, you’ll want to detect if a link fails entirely (e.g., ISP down).
Monitor “last active” timestamps or consecutive loss on each link. The custom dispatcher
implementation typically includes a failover timeout – e.g., if no packets have been successfully
sent on link X for a certain duration, it can drop that link’s weight to 0 until it recovers . In our
example plugin, there’s a failover-timeout property and logic to handle link health and
hysteresis so that traffic isn’t switched away too abruptly or oscillating . A best practice is to
wait a brief period to confirm a link is bad before declaring it down, and similarly to reintroduce it
cautiously (warm it up) when it comes back, to avoid flapping.
18 19
11
•
20 21
22
•
23
23
16
24
25 26
•
26
27 28
3
Hysteresis and smoothing: Network conditions can fluctuate rapidly, so it’s wise to smooth the
metrics over time (e.g. use an EWMA – exponentially weighted moving average – for packet loss or
RTT) and require a significant change before readjusting weights . The example dispatcher
uses EWMA by default and a switch-threshold (minimum ratio change) to avoid jittery
rebalancing . Recommendation: Use a reasonable stats polling interval (e.g. 500ms to a few
seconds) for adjusting link weights, and avoid reacting to every single out-of-order or NACK event. A
too-fast reaction can cause ping-pong effects in load allocation.
Adaptive Bitrate Control Integration
In addition to balancing links, an effective bonded setup can benefit from adaptive bitrate (ABR) control –
dynamically tuning the encoder’s bitrate based on network capacity. If all links together still can’t handle the
current bitrate (as evidenced by persistent packet loss and retransmissions), lowering the bitrate will
improve stream stability. Conversely, if the network is healthy with headroom, bitrate could be increased to
improve quality.
GStreamer integration: You can implement ABR by monitoring the same RIST statistics and adjusting the
encoder element’s bitrate property at runtime. The user’s plugin provides a dynbitrate element for
this purpose: it’s a pass-through controller that takes in an encoder element and the ristsink (or
dispatcher) as properties, and periodically checks network stats to decide if the bitrate should step up or
down . The control algorithm is typically a PID-like or threshold-based loop : for example, target a
maximum loss rate (say 0.5–1%). If actual loss exceeds the target (network struggling), dynbitrate will
decrement the bitrate in gentle steps; if loss is near zero and RTT is low, it may increment bitrate up to a
configured max, all within defined bounds and intervals. The goal is to converge to the highest bitrate that
the network can sustain without excessive packet loss.
Key considerations for ABR:
Use RIST feedback: Packet loss percentage (or NACK count) is a direct indicator of overload. RTT can
indicate growing queues; a sudden RTT spike might precede loss, so it can be used as a congestion
signal. For instance, if RTT climbs beyond a target (say 100 ms), it might trigger a hold on increasing
bitrate or even a reduction . The dynbitrate element in the example allows setting a target
RTT and loss% to aim for .
Step sizing and limits: Changes to bitrate should be capped and gradual. Configure minimum and
maximum bitrate to avoid over-compression or saturating links, and a step size (absolute or
percentage) for each adjustment period . The dynbitrate in our case uses a percent step (e.g.
10% adjustments) and absolute caps . It also enforces a minimum interval between adjustments
(e.g. check every 2 seconds) to give the network time to reflect changes before the next tweak
.
Keyframe considerations: When reducing bitrate significantly, video quality will drop until the next
keyframe refreshes the frame quality. It may be beneficial to request a keyframe (IDR frame) when a
large downscale happens so that artifacts don’t persist. The plugin’s dispatcher has an option to
duplicate keyframes on all links – in load-sharing mode this means sending IDR frames
over every path. This improves recovery if a link was temporarily starved or if a switch occurs,
•
29 28
30 31
32 33 33
•
34 33
34
•
35
35
36
33
•
16 37
4
ensuring all paths have that crucial frame. Similarly, the dynbitrate element can force a keyframe
when stepping down ( downscale-keyunit=true ) so the encoder immediately delivers a fresh
frame at the new lower bitrate . This is a recommended practice to speed up convergence
after big bitrate cuts.
Coordinating with load balancing: A potential pitfall is if the link load-balancer and bitrate
controller work at cross purposes. For example, imagine one link becomes weak – the dispatcher
shifts traffic away from it, but an independent bitrate controller only sees total loss improving
(because you moved traffic) and might erroneously decide to increase bitrate, which could then
overload the remaining link. To avoid such conflicts, coordination is key. One approach is to
designate one system as primary: e.g., let the dispatcher handle distribution and expose an overall
“network health” metric that the bitrate controller uses. In the user’s design, the dynbitrate
element can take a reference to the ristdispatcher and coordinate decisions . When
working together, typically the dispatcher’s auto-balance might be turned off and the dynbitrate
logic will manage both bitrate and link weights in tandem . Alternatively, you could let the
dispatcher handle short-term load tweaks while a higher-level controller monitors long-term trends
to adjust bitrate. The main point is: ensure your adaptive bitrate algorithm is aware of multi-link
behavior – either track per-link stats or overall stats in a consistent way – so that it doesn’t keep
pushing bitrate up when one path is continuously being sidelined due to issues.
Configuration Tips and Potential Pitfalls
Pipeline setup: Use GStreamer’s bonding properties rather than manually constructing separate pipelines
wherever possible. The built-in approach (single ristsink / ristsrc with multiple addresses) ensures
proper synchronization of sequence numbers and ARQ logic . If you were to manually use multiple
ristsrc elements feeding a funnel or similar, you might inadvertently break the unified jitter buffer,
leading to duplicate or unordered requests. Stick with one ristsrc and let it internally manage multiple
sockets. On the sender, if using a custom dispatcher, insert it upstream of the ristsink sub-elements (as
shown in examples: ... ! ristdispatcher name=d ! ... d.src_0 ! ristsink uri=...
d.src_1 ! ristsink uri=... ) . This explicit pipeline approach is an alternative to using the
dispatcher property – it allows you to see and configure the dispatcher element directly. Just remember
to request enough source pads on the dispatcher (one per link) and link each to a ristsink pointing at a
different address.
Initial configuration: It’s wise to assign initial weights reflecting each link’s capacity – for example, if one
link has double the bandwidth of another, you might start with weights 0.66, 0.34 (approx two-thirds of
packets on the faster link) . If using the built-in round-robin , initial weights are essentially equal
(since it just alternates packets evenly). In critical applications, some recommend using broadcast
(duplicate) for keyframes or a small percentage of packets even in bonding mode, as a hedge against link
failure. The custom dispatcher in this case could be configured to duplicate certain strategic packets (e.g.
every keyframe or every Nth packet) across all links – a form of hybrid strategy. This isn’t built-in to
GStreamer but is provided in the Rust plugin ( keyframe-duplicate=true ) . Use it if you can
afford a slight overhead for much faster recovery from a sudden link drop.
Monitor and tune buffers: Bonding usually implies different network paths, which can have different
latencies. Set the reorder-section (jitterbuffer reordering window) large enough to cover the
38 39
•
40 41
42 41
12
43 44
45
16 37
5
differential delay between the fastest and slowest link. For example, if one path’s latency is up to 100 ms
more than the other’s, the receiver’s reorder buffer must be >100 ms deep; otherwise, packets from the
slower link might be seen as “lost” and trigger needless retransmits. By default, RIST might use a small
reordering window for low-latency, but when bonding, a bit more tolerance can drastically reduce
unnecessary retransmissions. There’s a trade-off: larger buffers add latency. Try to strike a balance based on
measured link delay differences. Start with defaults and increase if you see a lot of out-of-order warnings or
dupe retransmits in logs.
Also consider OS-level socket buffer sizes. With multiple high-bandwidth links, the incoming UDP buffer can
overflow if too small. It’s recommended to increase kernel receive buffers ( net.core.rmem_max and
rmem_default ) to handle peak traffic and re-transmissions . The RidgeRun guide suggests values like
10MB for rmem_max for high bitrate streams . This prevents packet drops in the OS that RIST cannot
recover (since those would never reach the jitterbuffer).
Network path diversity: Ensure the multiple links truly have independent failure modes (different ISPs or
routes), otherwise bonding won’t help much. If they share a bottleneck upstream, no amount of bonding
logic can avoid a joint outage. Also, if one link has significantly higher latency, consider using it in backup
role (very low weight that only ramps up if primary degrades). RIST bonding allows such strategies – e.g.,
keep a cellular link “hot but idle” at maybe 5% traffic, so its RTCP stays active and you have instant failover
when needed .
Testing and observation: Leverage the stats and logging. The custom elements can emit metrics on the
GStreamer bus (for example, the ristdispatcher posts a structured message “rist-dispatcher-stats” with
per-link metrics periodically ). Use these to log or display current RTTs, loss%, throughput on each
path. This real-time visibility is invaluable for tuning your weights, thresholds, and verifying that your load
balancing is working as expected. GStreamer’s GST_DEBUG logs for RIST can also show when NACKs are
sent, etc., but those can be quite verbose.
Pitfalls to avoid:
- Using multiple receivers incorrectly: Don’t try to recombine flows outside of the RIST plugin unless absolutely
necessary. If you must (e.g. older GStreamer without bonding support), use an rtpfunnel element (which
is designed to merge RTP streams) rather than a basic funnel, and feed that into a single
rtpjitterbuffer . But ideally upgrade GStreamer and use bonding-addresses – it’s far simpler and
more robust.
- Over-aggressive bitrate: RIST will retransmit lost packets, but if you push the encoder to generate more
bitrate than all links can handle combined, you’ll end up in a constant loss/retransmit cycle, adding latency
and eventually failing if jitter buffers overflow. Aim for a target utilization (e.g. 80-90% of combined capacity)
to leave room for retransmissions and bursts. Dynamic bitrate control should enforce a cap – for example,
the dynbitrate element’s max-bitrate property can be set to the known sum of link bandwidths .
- Simultaneous link failure: If two links fail at once (or one fails in seamless mode), playback can still glitch if
no path delivers a packet. Bonding improves resilience but isn’t magic if all paths drop a given packet.
Ensure max-rtx-retries is tuned (the number of times to retry a lost packet) – the default 7 is usually
fine , but in high congestion you might briefly need more retries. - Latency vs. loss trade-off: You can
configure receiver-buffer (total buffer) and reorder-section in ristsrc . Larger buffers tolerate
more jitter and loss (giving more time for retransmits) at the cost of latency. For live streaming, keep latency
as low as feasible, but if you see continual losses, increasing buffer a bit might be necessary. This is not
specific to bonding, but with bonding you often have disparate link qualities, so a bit more buffering can
46
46
47
48 49
34
50
6
smooth things out. - Compatibility and encryption: Note that GStreamer’s current RIST plugin does not
implement the full Main Profile features like encryption or tunneling . If you require those, you might
need an external solution or wait for plugin updates. In bonding setups, encryption (DTLS or PSK) might
reduce performance or complicate using custom dispatchers outside the bin, so plan accordingly (e.g., test
with libRIST library if needed for advanced cases).
Community guidelines and support: The RIST community (e.g. the RIST Forum and Video Dev Slack)
suggests testing both bonding modes. Seamless (duplicate) mode is often favored for mission-critical
feeds where bandwidth is less an issue – it’s simpler since the receiver just picks the first-arriving packet and
duplicates are dropped, resulting in effectively zero packet loss if at least one path delivers every packet
. Bonding (load-share) mode is valued when you truly need combined throughput or want to leverage
every bit of capacity; just be prepared for more tuning. Common recommendations include sending a low
bitrate test stream or even just RTCP-only traffic on backup links to measure their RTT/loss continually, so
your system can detect a bad link before it’s heavily used. Also, many practitioners schedule periodic
keyframes (~2 seconds) so that if recovery happens after loss, the decoder refreshes quickly. GStreamer’s
encoder element property key-int-max can be used to limit GOP length (e.g. key-int-max=60 for a 2s
interval at 30fps) – shorter GOPs are friendlier to error recovery in any ARQ protocol.
Finally, always run real-world tests for your specific scenario. Try pulling one network cable at a time to
simulate failures, observe how quickly the pipeline recovers and whether any artifacts occur. Check the
GStreamer mailing lists or forums for any quirks in the RIST elements – for example, early versions required
setting cname or even/odd ports correctly. Stay updated with the latest GStreamer, as bonding support
was an “upcoming” feature in initial releases and has matured in recent versions. By following these
guidelines – using the proper bonded pipeline structure, leveraging stats for smart load balancing, and
integrating adaptive bitrate – you can achieve a very robust streaming pipeline with RIST in GStreamer.
Sources:
Collabora RIST announcement – “GStreamer support for the RIST Specification”
RIST Forum article on Multi-Link (Ciro Noronha)
RidgeRun Developer Wiki – Using RIST with GStreamer (bonding config examples)
RIST Elements Plugin Documentation (Rust GStreamer bonding plugin by user)
GStreamer RIST plugin reference and examples (Gst 1.24 bonding with custom dispatcher
option)
RidgeRun guide – Optimizing Network Settings for RIST (kernel buffer advice)
Multi-Link Operation Using RIST — RIST Forum
https://www.rist.tv/articles-and-deep-dives/2022/5/9/multi-link-operation-using-rist
GStreamer support for the RIST Specification
https://www.collabora.com/news-and-blog/news-and-events/gstreamer-support-for-the-rist-specification.html
ristsink - GStreamer
https://gstreamer.freedesktop.org/documentation/rist/ristsink.html
Reliable Internet Stream Transport (RIST) with GStreamer
https://developer.ridgerun.com/wiki/index.php/Reliable_Internet_Stream_Transport_(RIST)_with_GStreamer
51
1
52
53
• 12 13
• 1 2
• 9 11
• 16 32
• 10 45
• 46
1 2 3 47 52
4 5 6 12 13 14 17 53
7
8 9 10 11 46 51
7
README.md
https://github.com/RephlexZero/rist-bonding/blob/4e82a1f1246086b88455b4702ef0ae6d82cfc8fd/docs/plugins/README.md
README.md
https://github.com/RephlexZero/rist-bonding/blob/4e82a1f1246086b88455b4702ef0ae6d82cfc8fd/crates/rist-elements/
README.md
ristsrc: GStreamer Bad Plugins 1.0 Plugins Reference Manual
https://people.collabora.com/~nicolas/rist/html/gst-plugins-bad-plugins-ristsrc.html
dispatcher.rs
https://github.com/RephlexZero/rist-bonding/blob/4e82a1f1246086b88455b4702ef0ae6d82cfc8fd/crates/rist-elements/src/
dispatcher.rs
dynbitrate.rs
https://github.com/RephlexZero/rist-bonding/blob/4e82a1f1246086b88455b4702ef0ae6d82cfc8fd/crates/rist-elements/src/
dynbitrate.rs
15 16 23 26 32 33 34 35 36 37 40 41 42 45
18 19 43 44 48 49
20 21 22 50
24 25 27 28 29 30 31
38 39
8