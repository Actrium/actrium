# WebRTC v0.20 Recovery Test Plan and Release-Gate Matrix

Status: Implemented T1/T2/T3 plan; system/device rows remain release gates
Date: 2026-08-05
Related: `rfcs/0504-webrtc-v020-rtc-controller-architecture.md` §Stage 5 items
10-12, `docs/webrtc-v020-test-contract.md` (T1-T3 semantics),
`docs/webrtc-v020-test-plan.zh.md` (full layered/network scenario plan),
`docs/webrtc-v020-clock-patch-tracking.md` (R3 clock decision)

The recovery mechanisms and layered fault seams are implemented. This document
records which assertions run deterministically in-repo and which require real
TURN, system-network, browser, or Android/iOS environments.

## 1. Harness inventory (what exists after Stage 5)

| Harness | Scope | Entry points |
| --- | --- | --- |
| Reducer/controller unit tests | Pure state-machine recovery semantics (no wall clock, no network) | `peer/reducer_tests.rs`, `peer/controller_tests.rs` |
| T2 contract fixtures (real PC, loopback) | Real v0.20 PeerConnection pairs over loopback with the in-memory signaling hub | `interop_tests.rs`, `wiring_tests.rs`, `validation_tests.rs` |
| `RuntimeFaultProbe` (S5-10) | Socket/task/timer boundary metrics + connect-fail / UDP-blackhole / driver-exit injection | `fault.rs`, injected via `RtcRuntimeConfig::with_runtime_override` |
| `DeterministicNetwork` (S5-10) | Deterministic signaling loss/delay/duplicate/partition rules | `fault.rs`, wraps any `SignalingTransport` |
| `RtcRecoverySupervisor` (S5-1) | Network/app-lifecycle fact injection (network down/up, address change, background/foreground, force-reconnect) | `RtcConnectionManager::supervisor().publish(..)` |
| Unified observation (S5-11) | Per-peer snapshot + outstanding bytes + lane count + convergence; node totals | `RtcConnectionManager::observe()` |
| System harness | Real-link faults: Linux netns/`tc`, Loran TURN, browser | release/CI environment outside the unit-test process |
| Platform harness | Android/iOS device network + app lifecycle | L5 mobile E2E harness |

## 2. Use-case transfer matrix

| # | Use case | Lands in | Preconditions / notes |
| --- | --- | --- | --- |
| 1 | Wi-Fi ↔ cellular switch (address change → Soft Recovery) | Platform harness (Android/iOS) primary; reducer tests cover the policy (`LocalAddressChanged` starts same-generation ICE recovery) | Platform: drive `LocalAddressProvider` override + supervisor publish; assert `observe()` preserves `pc_generation`, advances `ice_generation`, the Runtime socket observation sees the changed address, and the peer is re-`sendable`; failed restart may escalate to Hard |
| 2 | Offline → online recovery (soft ICE restart first) | T2 contract fixture: `DeterministicNetwork` partition/heal on the signaling link + supervisor `NetworkUnavailable`/`Available` | Assert Soft first (ice_generation+1, same pc_generation), escalation to Hard on restart failure; reducer tests cover the decision tree (S5-2/S5-3) |
| 3 | UDP blackhole mid-session (media path dead, signaling alive) | T2 contract fixture: `RuntimeFaultProbe.set_blackhole_udp` on one PC's socket | Assert: sends stop erroring immediately (channel accepts), recovery triggers via PC disconnected events; observation shows `Recovering` |
| 4 | TURN credential refresh (`set_configuration`) | System harness with Loran TURN (real TURN server) | Rejected `set_configuration` → Hard Recovery is reducer-tested (S5-6); live TURN expiry uses Loran TURN |
| 5 | NAT rebinding / mapping timeout | System harness (netns + `tc` + NAT device/VM) | Cannot be emulated at the Actr seam (RFC §"Runtime and network-test APIs") |
| 6 | App background → foreground (mobile) | Platform harness (Android/iOS) | Policy reducer-tested (S5-5); platform drives `AppBackground`/`AppForeground` via supervisor |
| 7 | Fleet recovery isolation (N peers recover at once) | T2 contract fixture: N controller pairs + supervisor broadcast; builds/restarts remain peer-owned | Assert a stalled peer does not block other peers and every controller converges within the shared test deadline |
| 8 | Mailbox overflow under event storm | Reducer/controller tests plus restored integration event-storm suites | Overflow → Hard Recovery, `mailbox_overflows` observable; queue count/bytes are finite |
| 9 | close-all under partial network partition | T2 contract fixture: partition one peer's signaling, `close_all()` must still converge within the overall deadline (S5-8) | Assert `observe().peer_count == 0` after deadline |
| 10 | Deterministic policy replay | T1 `ManualPolicyClock` | Stock v0.20 has no `Runtime::now()` seam; high-level and bit-exact protocol replay are not claimed |
| 11 | Loss/delay/reorder at the *protocol packet* level | Optional Sans-I/O lab | Clock level C remains deferred and is not claimed by `runtime-mock`; high-level loss/blackhole coverage is already present |
| 12 | Mixed-fleet interop (legacy peer ↔ v0.20 peer) | External release gate only when a deployment actually mixes versions | Actr message framing remains byte-compatible through the shared codec; no legacy RTC implementation remains in production dependencies |
| 13 | Initial Offer/Answer/PC readiness never converges | T1 manual clock + real-PC transport wiring tests | One 9.5 s transport-epoch deadline closes the exact generation; `DefaultWireBuilder` delegates to WirePool; three retries allocate monotonically fresh PC generations without detaching the Connecting flight; message-driven Offerer and Answerer cases fail builds 1/2, succeed on generation 3, and deliver once |

## 3. Rules for the follow-up plans

- Recovery *policy* assertions belong in reducer/controller tests (deterministic,
  no network). Only *mechanism* integration belongs in T2/system/platform plans.
- Every new scenario MUST assert via the unified observation surface
  (`observe()` + snapshot subscription), not internal state.
- Fault injection stays out of production builds (RFC §"Security and privacy"):
  `RuntimeFaultProbe`/`DeterministicNetwork` are `cfg(test | test-utils)` only.
- TURN credentials never appear in logs, snapshots, or metrics labels
  (RFC §"Security and privacy").
