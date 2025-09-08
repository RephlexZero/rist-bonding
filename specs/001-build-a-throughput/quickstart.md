# Quickstart (Phase 1)

This quickstart demonstrates running a simulated multi-link contribution and forwarding using the Rust workspace and network-sim.

## Prerequisites
- Running in the provided dev container (Ubuntu 24.04 LTS)
- Rust toolchain installed in container

## Steps
1. Bring up simulated cellular links (2-4) using `network-sim` (namespaces + netem)
2. Start the bonding transmitter (CLI) targeting bitrate, pointing at receiver forwarder
3. Start the receiver forwarder and configure one or more destinations
4. Observe metrics and simulate link failures or degradations

## Expected Outcome
- Contribution stream remains continuous while links fluctuate
- Adding/removing destinations on the receiver forwarder does not interrupt the contribution feed
