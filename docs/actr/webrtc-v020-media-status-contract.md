# WebRTC v0.20 Media / Status / Binding Contract Seams (Stage 6 item 10)

Status: contract of record for T2-T3 media/status/binding tests
Date: 2026-08-06
Related: `rfcs/0504-webrtc-v020-rtc-controller-architecture.md` §Stage 6,
`docs/webrtc-v020-test-contract.md`, `docs/webrtc-v020-recovery-test-plan.md`

This document pins the **contract seams** Stage 6 established for media, the
status API, and binding observation, and says where concrete media/platform
use cases land. Per the RFC, no specific media/device matrix is built at this
stage.

## 1. Track ownership (media) contract

- Every remote track is owned by exactly one `PeerConnectionSession`
  (generation-bound) and polled by exactly one pump task
  (`v020/media.rs::track_pump`) — the single-consumer rule, mirroring the
  DataChannel lane pump. `Session::close()` stops each owned track
  (`Track::stop()`) before closing the PeerConnection.
- `TrackAdded` on a stale generation never registers (controller intercept +
  reducer stale-count, invariant #5).
- RTP payloads dispatch as `MediaSample`s to the node `MediaFrameRegistry`
  with the same labels as the 0.14 stack: codec = uppercased MIME suffix
  (`audio/opus` → `OPUS`), media type from the `audio/`/`video/` prefix.
- `add_track`/`remove_track` execute serialized on the controller's event
  loop against the active generation and answer via oneshot; renegotiation
  rides the existing `NegotiationNeeded` → offer path (one offer, merged
  while in flight). Verified: `add_track_on_active_session_returns_sender`,
  `add_track_without_active_session_is_not_ready`,
  `negotiation_needed_after_topology_change_produces_one_offer`.
- The factory registers the default codec set + interceptor registry on
  every build (S6-7b); verified by
  `factory_registers_default_codecs_and_interceptors` (offer SDP carries
  rtpmap VP8/H264).

## 2. Stats → NetworkPath contract

- `get_stats(Instant::now(), StatsSelector::None)` is sampled once per
  `Sendable` entry (initial connect, recovery, pending switch) — never
  polled per send, never fleet-wide at once.
- `network_path` classification: either selected-pair side `Relay` →
  `Relayed`; a clear pair → `Direct`; missing/inconsistent report keeps the
  previous value. `network_path` NEVER gates `sendable`.
- Upstream quirk pinned: rtc 0.20 keys candidate entries `<Type>_<id>` while
  the selected pair references the bare id — extraction matches by key
  suffix (`v020/stats.rs::candidate_type_of`).
- Verified: classification unit matrix + `sendable_snapshot_resolves_
  network_path_via_stats` (real loopback pair reports Direct).

## 3. Status API + initial replay contract

- `ActrRef::get_rtc_peer_status(peer)` / `list_rtc_peer_statuses()` /
  `subscribe_rtc_peer_status(peer)` return the public
  `actr_framework::WebRtcPeerState`.
- Subscribe replays the current state immediately (a late subscriber never
  waits for a change); `revision` is monotonic per peer for de-duplication.
- `relayed` is derived from `network_path` and kept one compat cycle
  (additive-first migration; new consumers read `network_path`).
- Verified: `subscribe_public_replays_initial_then_forwards`,
  `to_public_maps_network_path_to_relayed`, manager list/subscribe tests.

## 4. Hook mapping contract

- The projection is transition-driven (S6-6b): a per-peer forwarder consumes
  the controller's snapshot watch; no polling interval. The manager's
  peer-registry version watch triggers re-sweeps on controller
  create/remove.
- Unified hook: `on_webrtc_state_changed(WebRtcPeerState)` fires on every
  revision advance; a late-installed workload receives the current state on
  the first pass.
- Legacy mapping (0.14 parity, pinned by the `projection_*` unit matrix):
  ConnectStart on every transition into `Connecting` (including recovery
  re-entry); Connected on the sendable edge (relayed from `network_path`);
  Disconnected on entry into `Idle`/`Recovering`, with the
  Recovering→Idle double-fire ordering preserved.
- Guest ABI boundary: the unified hook reaches linked/FFI observers; the
  WASM/DynClib guest contract (WIT + guest_abi) keeps the three legacy
  hooks until the Stage 7 contract update (dispatch-filtered, fail-loud on
  a contract slip).

## 5. Binding observation surface

- FFI (`crates/actr-ffi`): `WebRtcPeerStateBridge` (+ network-path /
  unavailable-reason enums), `WebRtcObserverBridge::on_state_changed`,
  `ActrRefWrapper` get/list/subscribe with a `RtcPeerStateListener`
  callback + drop-to-stop `RtcPeerStateSubscription`. The `webrtc-v020`
  feature gates the bodies (uniform UniFFI surface; `None`/empty on 0.14
  builds).
- Swift: `WebRTCPeerState` / `WebRTCNetworkPath` / `WebRTCUnavailableReason`
  public types, `WebRTCObserver.onStateChanged` (default no-op — source
  compatible), `ActrRef` get/list/subscribe with a closure listener.
- Kotlin: regenerated `actr.kt`; dsl aliases (`WebRtcPeerState`,
  `WebRtcUnavailableReason`).
- TypeScript/Web: no webrtc hook surface exists today — nothing to update;
  their exposure is a Stage 7 guest-contract item.

## 6. Deferred to subsequent test plans

- Real media interop (RTP round-trip through `track_pump` on a live pair,
  RTCP feedback behavior) → T2 contract fixtures with the S5-10 fault
  seams; then system/platform harnesses per
  `docs/webrtc-v020-recovery-test-plan.md`.
- Media renegotiation under glare/concurrent add-remove → T2 fixtures
  (the reducer-level dedup is pinned; the wire-level matrix is not built).
- Binding-level e2e (Swift/Kotlin app driving the new status APIs on
  device) → platform harness (CI e2e covers build + the legacy hooks).
