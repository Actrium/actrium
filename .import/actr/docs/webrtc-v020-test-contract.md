# webrtc-rs v0.20 Test Contract (T1/T2/T3) and Test-Migration Inventory

Status: Implemented; maintained as the v0.20 test-layer contract
Date: 2026-08-04
Contract source: `rfcs/0504-webrtc-v020-rtc-controller-architecture.md` §"Test
structure based on v0.20 layered architecture" (lines 1375-1508)
Scope: define the T1/T2/T3 proof layers and their ownership, the
version/owner/teardown rules for the T2↔T3 shared semantic contract, and the
inventory mapping every pinned pre-cutover behavior to its destination in the
new structure.

This is a contract and an inventory, not a scenario matrix. The detailed
parameterized scenarios and network-test architecture are defined in
[`webrtc-v020-test-plan.zh.md`](webrtc-v020-test-plan.zh.md).

---

## 1. Layer definitions and ownership

Production has exactly one path; the test layers only observe or drive different
boundaries of that path. They are proof layers, **not** replaceable WebRTC
backends — every layer reuses the same `RtcPeerController`, `RtcPeerSnapshot`,
Hook, Lane, and error semantics.

```text
RtcConnectionManager
└── RtcPeerController (one per remote peer)          <- T1 owns this boundary
    └── RtcPeerEffectPort (crate-private)            <- effect contract
        └── WebRtcEffectRunner (only production impl) <- T2 owns this boundary
            └── PeerConnectionSession (one per generation)
                └── Arc<dyn PeerConnection> (webrtc-rs v0.20 high-level API)
└── RtcConnectionManager routing + multi-peer wiring <- T3 owns this boundary
```

| Layer | Minimal fixture | Proves (owns) | Does **not** prove |
| --- | --- | --- | --- |
| **T1 Controller** | `ControllerTestHarness`, `TestEffectSink`, `ManualPolicyClock` | reducer purity, Controller task, mailbox, effect identity (`peer_id`/`pc_generation`/`operation_id`), Snapshot/Hook derivation | the v0.20 API, protocol packets, real network |
| **T2 v0.20 API** | `validation_tests`/`interop_tests` real loopback pairs and `RuntimeFaultProbe` | `PeerConnectionBuilder`, `WebRtcEventHandler`, `PeerConnection`/`DataChannel`/`Track` `poll()`, backpressure, stats, explicit close | system network and mobile lifecycle |
| **T3 Actr/environment** | `test_support::rtc_harness::TestHarness`, integration/system/platform harness | Manager routing, multi-peer isolation, business sendability, real network/platform behavior | exhaustive reducer combinations or every protocol packet detail |

Hard rules (from the canonical RFC):

1. **T1 never creates a PeerConnection.** It drives the pure reducer plus a
   `TestEffectSink` double that records effects and injects completions/facts.
2. **T2 uses the real v0.20 high-level API.** No fake PeerConnection at this
   layer. `SansIoProtocolRig` is an *optional* deterministic tool inside T2
   only — it is not a second production path and never becomes an effect runner.
3. **T3 uses the complete production Manager/Controller/Runner** wiring, with
   system or platform environments when the gate requires.
4. **The reducer is pure**: time is an explicit input or timer fact, never an
   implicit wall-clock read.

### Shared support (observation/scheduling only, no second state machine)

`ControllerTestHarness`, the real loopback pair helpers,
`test_support::rtc_harness::TestHarness`, `ManualPolicyClock`,
`RuntimeFaultProbe`, and deterministic signaling rules. Fixtures are
per-test-instance and MUST NOT share process-wide mutable fault flags, Runtime
profiles, Handler recorders, or logical clocks.

---

## 2. The T2↔T3 shared semantic contract

The "shared semantic contract" is the set of facts and semantics that **all
layers must agree on** so that a milestone asserted at one layer means the same
thing at another. It is:

- **The control state machine** (`RtcPeerPhase`: Idle / Building / Negotiating /
  TransportReady / Sendable / Recovering / Closing) and its allowed transitions.
- **The authoritative snapshot** (`RtcPeerSnapshot`): `peer`, `pc_generation`,
  `revision`, `status`, `sendable`, `network_path`, `unavailable_reason`,
  `signaling_dropped`.
- **The public status mapping** (`WebRtcPeerStatus`: Idle / Connecting /
  Connected / Recovering) and the **sendability** derivation (active generation
  ∧ PC-connected ∧ `RpcReliable` open ∧ not recovering ∧ not closing).
- **The error model** (`RtcUnavailableReason`, a stable FFI-safe enum) and
  signaling backpressure accounting (`signaling_dropped`).
- **Identity / staleness rules**: `pc_generation`, `operation_id`, `ice_generation`,
  `ufrag`; a fact/completion is applied only if its generation is live
  (invariants #5/#6).
- **Lane vocabulary**: the four `LaneKind`s and their exact wire labels
  (`RPC_RELIABLE` / `RPC_SIGNAL` / `STREAM_RELIABLE` / `STREAM_LATENCY_FIRST`)
  and per-lane QoS.

### 2.1 Source of truth per fact

| Contract fact | Source of truth (owner) | Verified by |
| --- | --- | --- |
| Phase transitions, sendable derivation, identity/staleness, close barrier, role/glare, overflow→unhealthy | `peer/reducer.rs` + `peer/state.rs` (the single-writer Controller) | T1 (pure reducer) directly; T2/T3 assert the *same* outcomes, never re-derive them |
| Snapshot shape, public status, `RtcUnavailableReason`, lane labels/QoS | `v020/status.rs` | T1; T2 via snapshot reads; T3 via Manager query/subscribe |
| v0.20 API behavior (build/handler/poll/backpressure/close) | webrtc-rs v0.20 itself | T2 (`validation_tests`, real-PC `interop_tests`) |
| Multi-peer routing, role arbitration, isolation, business wiring | `manager.rs`, `signaling/router.rs` | T3 (`test_support::rtc_harness`, integration tests) |

No layer re-implements the contract; each only *observes* it through its own
boundary. A milestone (e.g. "sendable becomes true") is defined once by the
reducer and read identically by T1 (snapshot), T2 (real PC drives the same
reducer), and T3 (Manager snapshot).

### 2.2 Versioning rule

The contract is versioned by the **snapshot revision** and the
**`RtcPeerSnapshot` shape** (plus the reducer phase set and the error enum). A
contract version bump is any change to: a phase, a snapshot field, the
sendability derivation, an error variant, an identity rule, or a lane
label/QoS.

Propagation rule: a contract version bump MUST move all three layers in
lockstep, in this order:

1. Change the source of truth (`peer/reducer.rs`/`state.rs`/`status.rs`).
2. Update T1 (pure reducer + `ControllerTestHarness` assertions) — the cheapest
   layer; the contract is proven here first.
3. Update T2 (v0.20 API / `HighLevelWebRtcPair` contract assertions).
4. Update T3 (`ActrRtcIntegrationRig` milestone assertions).

A bump that changes the public snapshot/status surface is breaking and follows
the project 0.x breaking-change process (additive first where possible: add the
new field/variant, keep the old for one compatibility window, then remove).

### 2.3 Owner rule

The **Controller (reducer + state) is the single owner** of the control-plane
contract. T2 and T3 do not own or fork it: they assert it through real PCs (T2)
and through full production wiring (T3). If T2 or T3 ever disagrees with T1,
**T1 (the reducer) wins** — the other layer's harness has a wiring bug, not a
contract difference.

### 2.4 Teardown rule

Teardown is owned by the layer that created the resource, and a rig MUST NOT
exit with a live PeerConnection, driver task, or socket:

- **T1**: nothing physical to tear down (no PeerConnection); the harness sends
  terminal Close and joins the Controller task/effect sink.
- **T2**: each `PeerConnectionSession` is explicitly closed by its owner
  (invariant #12: every PeerConnection is closed by its session). A
  Each real-PC fixture closes both PCs and asserts task/socket convergence.
- **T3**: `TestHarness` closes via `Manager::close_all` (drop
  command/lane senders, then join each controller task). Cross-layer ordering
  is: T3 harness tears down its controllers (which close their sessions/PCs),
  and only then is any T2-owned direct PC closed. A rig exit with a leaked
  PC/driver/socket is a test failure, not a warning.

---

## 3. Test-migration inventory (0.14 behavior → new structure)

Each pinned pre-cutover behavior maps to its current T1/T2/T3 proof. The exact
wire contract is shared with the transport fragment codec and lane vocabulary.

| # | 0.14 behavior (pinned by) | v020 coverage now | Destination rig | Status |
| --- | --- | --- | --- | --- |
| 1 | Lane → DataChannel wire labels (`charter_lane_data_channel_labels_are_stable`) | `status.rs`, `peer/lane.rs`, four-lane interop | T1/T2/T3 | Done |
| 2 | Lane QoS config — LatencyFirst unordered+rtx3, rest reliable (`charter_lane_data_channel_config_matches_qos_contract`) | `peer/lane.rs` metadata tests and real four-lane interop | T1/T2/T3 | Done |
| 3 | Lane → distinct slot identities | `LaneKind` mapping and active-lane registry tests | T1 | Done |
| 4 | `RpcReliable` readiness anchor (any-open → sendable) | `peer/reducer.rs` + `peer/controller.rs` lane pump; reducer/controller tests | T1 (sendable), T2 (real open), T3 | Done (anchor only) |
| 5 | Phase → public status/sendability mapping | `peer/state.rs`, reducer tests | T1 | Done |
| 6 | One-active-generation invariant | reducer + controller tests | T1 | Done |
| 7 | Stale event/completion rejection (invariant #5) | reducer + controller tests | T1 | Done |
| 8 | Close barrier / close-during-build (late success/failure) | reducer + controller tests | T1 | Done |
| 9 | Controller exits on terminal close (N1) | controller test | T1 | Done |
| 10 | Signaling backpressure — full queue never stalls peers (N2) | controller test (`full_signaling_queue_...`) | T1, T3 | Done |
| 11 | Role negotiation (server-arbitrated) + glare convergence | reducer role tests + `interop_tests::simultaneous_connect_glare_...` | T1 (role), T3 (glare) | Done |
| 12 | ICE generation/ufrag candidate tagging + stale rejection (invariant #6) | reducer ufrag tests | T1 | Done |
| 13 | Offerer/Answerer interop over real v0.20 PCs | `interop_tests::offerer_answerer_interop_reaches_sendable` | T2 (real PCs), T3 (full wiring) | Done |
| 14 | Multi-peer isolation (close one, others stay up) | `interop_tests::multi_peer_connect_isolated_...` | T3 | Done |
| 15 | Close + reconnect (teardown + fresh generation) | `interop_tests::close_peer_tears_down_...` | T3 | Done |
| 16 | Mailbox overflow → unhealthy → hard recovery (O1) | reducer + controller + mailbox tests | T1 | Done |
| 17 | Hook behavior (`on_webrtc_connecting/connected/disconnected`) | transition projection plus unified `on_webrtc_state_changed`; node-owned task | T1/T3 | Done |
| 18 | Signaling lifecycle (WS connect/reconnect/DisconnectReason/ReconnectConfig) | production WebSocket transport adapter + router/recovery tests | T3 | Done |
| 19 | DataChannel inbound routing + four lanes + reassembly budgets | session lane pump, bounded inbound router, four-lane and large-transfer tests | T2/T3 | Done |
| 20 | Soft/Hard recovery (ICE restart, address change, app fg/bg, TURN refresh) | reducer/controller recovery tests, address monitor, configuration acknowledgement | T1/T3 | Done |
| 21 | Mobile recovery invariants (Wi-Fi/cellular, offline/online) | common RTC integration harness plus Android/iOS L5 system harness | T3 | Implemented; device release gate remains environmental |
| 22 | Media (TrackRemote/RtpSender, renegotiation) | real track poll/add/remove/send interop tests | T2/T3 | Done |
| 23 | Snapshot query/list/subscribe + immediate replay + `on_webrtc_state_changed` | Controller-maintained internal/public watch channels, Manager/ActrRef API, hook projection | T1/T3 | Done |
| 24 | VNet / fault injection | `ManualPolicyClock`, `RuntimeFaultProbe`, deterministic signaling, real T3 link tests | T1/T2/T3 | Layered strategy; stock v0.20 has no driver clock seam |

---

## 4. Clock-boundary note (feeds Stage 5 / R3 tracking)

The published crates.io webrtc-rs v0.20.0 `Runtime` trait does not expose
`Runtime::now()`, and Actr now consumes that stock release without a local
backport. T1 still uses a per-fixture `ManualPolicyClock` for Actr policy
deadlines, but the high-level PeerConnection driver and protocol core are not
claimed to support virtual-time replay. Details and source-audit evidence are
tracked in `docs/webrtc-v020-clock-patch-tracking.md`.
