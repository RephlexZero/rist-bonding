# network-sim rework plan: crate-managed namespaces and shaping

Date: 2025-09-08
Scope: crates/network-sim

## Problem

Current tests (e.g., `crates/network-sim/tests/concurrent_bandwidth_validation.rs`) manually:
- create/delete namespaces with `ip netns add/del`
- move veth ends across namespaces
- apply qdisc via `tc` in a specific namespace using `ip netns exec`
- enter namespaces in-process using `nix::sched::setns`

Issues:
- Test duplication of imperative shell steps; brittle, harder to reason about
- Mixed approach (exec vs setns) scattered in tests
- No RAII lifetimes/cleanup; reliance on best-effort cleanup
- Harder to reuse in other test crates/applications

## Goals

1. Provide a first-class, safe(ish) Rust API in `network-sim` for:
   - Namespace lifecycle (create, enter, drop)
   - Veth creation, IP config, moving into namespaces, bring-up
   - Apply/clear qdisc (egress and ingress) per interface, optionally inside a namespace
   - Execute functions or commands inside a namespace
2. Make tests use the crate API only; no direct `ip` or `tc` shells or raw `setns` in tests
3. Ensure deterministic cleanup via RAII and explicit cleanup helpers
4. Preserve existing public API; add new higher-level API without breaking current callers

Non-Goals (for this iteration):
- Cross-platform support beyond Linux
- Rootless namespace creation (requires complex userns setup)

## Design

### New modules/types

- namespace::Namespace
  - Represents a Linux network namespace by name
  - API:
    - `Namespace::create(name: impl Into<String>) -> Result<Namespace>`
    - `Namespace::ensure(name: &str) -> Result<Namespace>` (create-if-missing)
    - `fn name(&self) -> &str`
    - `fn delete(self) -> Result<()>` (consumes self; Drop also best-effort deletes)
    - `fn enter(&self) -> Result<NamespaceGuard>`
    - `fn exec(&self, cmd: &str, args: &[&str]) -> Result<Output>` (uses `ip netns exec`)

- namespace::NamespaceGuard
  - RAII guard that switches the current thread into the target namespace and restores the original namespace on Drop
  - Impl:
    - Captures `/proc/self/ns/net` fd as original, opens `/run/netns/<name>` for target, calls `setns`
    - `Drop`: calls `setns` back to original
  - Requires nix = { features = ["sched"] }

- link::VethPairConfig (new, or extend existing `ShapedVethConfig`)
  - Fields:
    - `tx_if`, `rx_if`, `tx_ip_cidr`, `rx_ip_cidr`
    - `tx_ns: Option<String>`, `rx_ns: Option<String>`
    - `params: NetworkParams` (egress shaping on tx_if by default)
  - Methods:
    - `VethPair::create(&QdiscManager, &VethPairConfig) -> Result<VethPair>`

- link::VethPair (handle)
  - Holds names, optional Namespace handles (by name), and references to applied qdisc
  - Methods:
    - `fn tx_addr(&self) -> IpAddr`, `fn rx_addr(&self) -> IpAddr`
    - `async fn apply_egress(&self, qdisc: &QdiscManager, params: &NetworkParams)`
    - `async fn apply_ingress(&self, qdisc: &QdiscManager, params: &NetworkParams)`
    - `async fn clear(&self, qdisc: &QdiscManager)`
    - `async fn delete(self)` (drops veth and namespaces)
  - `Drop`: best-effort cleanup

- qdisc additions
  - `async fn get_interface_stats_in_ns(&self, ns: Option<&str>, iface: &str) -> Result<InterfaceStats>`
  - `async fn configure_interface_in_ns(&self, ns: Option<&str>, iface: &str, cfg: NetemConfig) -> Result<()>`
  - These wrap existing logic and prefix with `ip netns exec <ns>` as needed

### Convenience utilities (test-facing)

- `namespace::run_in_namespace<F, R>(ns: &str, f: F) -> std::thread::JoinHandle<std::io::Result<R>>`
  - Spawns a thread, enters `ns`, runs `f`, returns a handle
  - Useful for binding sockets in a specific namespace

- `helpers::udp`
  - Optional test utility module (behind `test-utils` feature) to spawn simple UDP sender/receiver within a namespace for throughput tests

### Feature flags and deps

- Move `nix` from dev-dependencies to normal dependencies under feature `ns-threads` (default = on for tests, off for minimal runtime)
  - `[features] ns-threads = ["nix/sched"]`
- Guard Linux-only code with `#[cfg(target_os = "linux")]`
- Provide clear errors on unsupported platforms

## Migration plan (tests)

Refactor `crates/network-sim/tests/concurrent_bandwidth_validation.rs`:

1. Replace manual setup with crate API
   - Use `Namespace::ensure(tx_ns)` and `Namespace::ensure(rx_ns)`
   - Create `VethPair` with both tx/rx namespaces and IPs
   - Apply rate via `qdisc.configure_interface_in_ns(Some(tx_ns), tx_if, ...)`

2. Replace `enter_netns` with `namespace::run_in_namespace` or `Namespace::enter()` in thread

3. Replace manual `tc -s` parsing with `QdiscManager::get_interface_stats_in_ns(Some(tx_ns), tx_if)`

4. Cleanup via `VethPair::delete()` (Drop does best-effort guard)

5. Keep capability gating (`has_net_admin`) early-exit unchanged

## Risks & mitigations

- Permissions (NET_ADMIN): tests already skip when unavailable
- RAII vs external tampering: if external processes alter namespaces while tests run, cleanup may fail; best-effort and idempotent cleanup mitigates
- Restoring namespace: must always capture original ns fd and restore in Drop; tests must avoid cross-thread guard sharing

## Work breakdown (tasks)

1. Add `Namespace` and `NamespaceGuard` (nix-based `setns`), feature-gated
2. Extend qdisc manager with `*_in_ns` methods
3. Introduce `VethPairConfig` and `VethPair` with create/apply/clear/delete
4. Optional `helpers::udp` under `test-utils` feature
5. Migrate `concurrent_bandwidth_validation.rs` to new API
6. Update README in `crates/network-sim` with examples and permissions note

## Acceptance

- All existing tests pass and no direct shelling to `ip/tc` remains in tests (except indirectly through crate)
- Setup/cleanup is one-liner per link with RAII cleanups
- Throughput results are within previous tolerances
