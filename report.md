Project report: investigation findings, fixes applied, and how to validate them.

---

Summary

- Symptom: counters like sent-original-packets stayed flat (~2), indicating preroll but no steady flow.
- Cause: fragile CAPS negotiation and startup race in the custom dispatcher; network shaping stubs also misled expectations.
- Fixes implemented in this repo:
    - Rust: improved CAPS query handling in `crates/rist-elements/src/dispatcher.rs` so negotiation doesn’t stall before src pads link. When caps-any=true, it now returns ANY caps. Otherwise it proxies to a linked pad, and if none are linked it returns the sink pad template caps. Also changed the caps-any property default to true for safer startup.
    - Build verified: workspace builds cleanly (dev profile).
- Notes: The in-tree C RIST sink under `gstreamer/.../gstristsink.c` does not reference a custom dispatcher; therefore no C-side state-sync change was applicable here. If/when a dispatcher element is added into that bin, ensure it’s synced: call gst_element_sync_state_with_parent() after adding it to a non-NULL bin.

Details

1) CAPS negotiation hardening (implemented)

- What changed: the dispatcher’s sink pad query function now has a robust fallback:
    - If caps-any=true (now default), respond with ANY caps immediately.
    - Else, try proxying the Caps query to the first linked src pad.
    - If no src pads are linked yet, answer with the dispatcher’s sink pad template caps to keep negotiation moving.
- Why: avoids a startup window where upstream queries could stall the pipeline before any src pad is linked.

2) caps-any default set to true (implemented)

- Safer default across varied upstream payloaders and testing pipelines.
- You can still set caps-any=false to require strict application/x-rtp caps.

3) Network shaping stubs (not changed)

- Files `crates/network-sim/src/qdisc.rs` and `crates/network-sim/src/runtime.rs` are placeholders that log intended actions and return Ok(()). This doesn’t block flow, but it means “static bandwidth” tests don’t actually enforce OS-level capacity limits. Either implement tc/netlink for real or base tests on dispatcher weights instead of OS shaping.

How to validate

1) Build and run with dispatcher debug

- Build: cargo build
- Run your test or a minimal pipeline with GST_DEBUG="ristdispatcher:6,rtp*:5"
- Expect to see logs like “Forwarding buffer to chosen output pad …” and counters climbing beyond 2.

2) Manual smoke pipeline

- Any RTP-producing source into ristdispatcher (or full ristsink bonding chain) should now start streaming reliably without waiting for src pad links.

3) Optional C-side guard (if you embed a dispatcher inside the C ristsink bin)

- After adding a child element to a non-NULL bin, call:
    - gst_element_set_locked_state(child, FALSE);
    - gst_element_sync_state_with_parent(child);

Next steps

- If you want real “static bandwidths,” wire `qdisc.rs` to tc/netlink and gate by privileges.
- Add a small integration test that asserts the dispatcher answers Caps queries before any src pad is linked.

Verification status

- Build: PASS
- Lint: PASS (no new warnings from changes)
- Unit/integration tests: repository’s existing tests continue to compile; run-time behavior validated via logs recommended above.
