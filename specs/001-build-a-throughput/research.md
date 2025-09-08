# Research for Cellular Bonding Feature (Phase 0)

## Unknowns and Clarifications
- Target end-to-end latency range: [NEEDS CLARIFICATION]
- Preferred codec/profile for contribution (H.264/H.265); audio codec and target bitrates: [NEEDS CLARIFICATION]
- Control surface location: transmitter UI vs receiver forwarder vs both: [NEEDS CLARIFICATION]
- Data cap/overage policy and thresholds per link: [NEEDS CLARIFICATION]
- Recovery time objective for outages: [NEEDS CLARIFICATION]
- Security/auth model for control and contribution: [NEEDS CLARIFICATION]

## Technology & Integration Notes
- Rust-first: Use existing workspace crates (`crates/rist-elements`, `crates/network-sim`).
- GStreamer + RIST plugin: Implement custom dispatcher behavior for bonding/aggregation.
- Simulation: Use `network-sim` to create Linux namespaces with netem to emulate loss/jitter/bandwidth caps; provide canned scenarios.

## Decisions
- Language: Rust (stable).
- Bonding strategy: Through custom dispatcher integrated with RIST elements; adaptive per-link weighting using observed throughput/loss/RTT.
- Telemetry: Structured logs + metrics surfaced via CLI/JSON.

## Alternatives Considered
- OS-level bonding (e.g., MPTCP): Rejected due to cross-carrier NAT and limited control over real-time adaptation.
- SRT instead of RIST: Out of scope; spec requires RIST.

## Next Steps
- Finalize unknowns above with stakeholder input.
- Outline data model and contracts assuming defaults; annotate areas with NEEDS CLARIFICATION.
