# webrtc-rs v0.20.0 Capability Matrix & Cutover Blockers

Status: Implemented and verified for the v0.20 production cutover
Date: 2026-08-04
RFC: `rfcs/0504-webrtc-v020-rtc-controller-architecture.md`
Scope: Records what webrtc-rs **v0.20.0** actually supports, verified against the
real crate source (not the RFC's assumptions), and the cutover blockers that must
be resolved before the v0.20 path can become the production backend. Every row
cites evidence.

This document is the RFC §"Configuration compatibility" deliverable: each
configured capability is marked **supported**, **unsupported**, **deprecated**, or
**blocked**, and the fail-fast policy for unsupported configurations is defined.

---

## 1. Verification method

The authoritative source is the exact crates.io `webrtc = 0.20.0` production
dependency, with no local source patch:

- `webrtc v0.20.0` (exact pinned crates.io release, published 2026-07-31; checksum in
  `Cargo.lock`). Features: `default = ["runtime-tokio"]`, plus `runtime-smol` and
  `runtime-mock`. Built on the Sans-I/O `rtc v0.20.0` core family (`rtc-ice`,
  `rtc-sctp`, `rtc-dtls`, `rtc-turn`, …) and `sansio v1.0.1`.
- Source inspected directly under the local Cargo registry
  (`~/.cargo/registry/src/…/webrtc-0.20.0`, `…/rtc-0.20.0`).
- Release notes: <https://github.com/webrtc-rs/webrtc/releases/tag/v0.20.0>
  (changelog range is v0.17.0 → v0.20.0).
- TURN/TCP tracking issue: <https://github.com/webrtc-rs/webrtc/issues/848>
  (**open**, `pre-1.0` feature, opened 2026-08-01).

The RFC's execution-model claims were **confirmed accurate** against the real
API (§2). The gaps are concentrated in socket/mux injection, TURN transports, and
virtual-network testing (§3, §4).

---

## 2. Confirmed v0.20.0 API surface (matches the RFC design)

| RFC claim | Real v0.20.0 API | Evidence |
|---|---|---|
| Sans-I/O `rtc` core + high-level async driver | `webrtc` is a thin async layer over the Sans-I/O `rtc` core; a `PeerConnectionDriver` background loop is spawned per connection | `webrtc-0.20.0/src/lib.rs` architecture docs |
| `PeerConnectionBuilder` injects a unified handler at build time | `PeerConnectionBuilder::with_handler(Arc<dyn PeerConnectionEventHandler>)`, consumed by `build()` | `peer_connection/mod.rs:297,408` |
| Object-safe `PeerConnection` | `trait PeerConnection: Send + Sync + 'static`; `build()` returns `impl PeerConnection`, wrap as `Arc<dyn PeerConnection>` | `peer_connection/mod.rs:474-542` |
| Object-safe `DataChannel` | `trait DataChannel: Send + Sync + 'static`; `create_data_channel` returns `Arc<dyn DataChannel>`; `on_data_channel(Arc<dyn DataChannel>)` | `data_channel/mod.rs:65-203` |
| Explicit `poll()` event consumption | `DataChannel::poll() -> Option<DataChannelEvent>` (`OnOpen/OnMessage/OnClosing/OnClose/OnError/OnBufferedAmount{Low,High}`) | `data_channel/mod.rs:199-254` |
| `try_send` / `writable` / `outstanding_bytes` back-pressure | All present; opt-in per-channel via `PeerConnectionBuilder::with_data_channel_send_buffer_limit` | `data_channel/mod.rs:113-198`, `peer_connection/mod.rs:402` |
| Send buffer limit | `with_data_channel_send_buffer_limit(usize)`; default `usize::MAX` (unbounded); `0` also unbounded | `peer_connection/mod.rs:379-405` |
| SCTP receive window configurable | `with_sctp_receive_buffer_size(u32)`; default 1 MiB; floor 1500 B | `peer_connection/mod.rs:259` |
| Runtime abstraction | `trait Runtime` (object-safe); `default_runtime() -> Option<Arc<dyn Runtime>>`; `TokioRuntime` under `runtime-tokio` | `runtime/mod.rs:91,426` |
| Bounded shared reactor pool | `with_dedicated_reactor_thread(bool)` + `with_reactor_pool_size(usize)`; process-global lazy pool, first size wins | `peer_connection/mod.rs:346,374` |
| Explicit close + resource ownership | `PeerConnection::close()`; sets `closing`, wakes send back-pressure, aborts the driver handle | `peer_connection/mod.rs:869-922` |
| ICE restart | `PeerConnection::restart_ice()` and `RTCOfferOptions { ice_restart: true }` | `peer_connection/mod.rs:1031`, `offer_answer_options.rs:47` |
| NAT 1:1 | `SettingEngine::set_nat_1to1_ips(Vec<String>, RTCIceCandidateType::{Host,Srflx})` | `setting_engine.rs:673` |
| IPv4/IPv6 selection | `SettingEngine::set_network_types(Vec<NetworkType>)` (`Udp4`, `Udp6`, `Tcp4`, `Tcp6`) | `setting_engine.rs:610` |
| ICE credentials (ufrag/pwd) | `SettingEngine::set_ice_credentials(ufrag, pwd)` | `setting_engine.rs:782` |
| mDNS | `MulticastDnsMode` (default **Disabled**); `SettingEngine::set_multicast_dns_mode` | `peer_connection/mod.rs:182`, `setting_engine.rs:729` |
| Codec registration / MediaEngine | `MediaEngine::default()` + `register_default_codecs()` or manual `register_codec` | `media_engine.rs:336` |
| Interceptor registry | `Registry::new()` / `Registry::default()` (NoopInterceptor) / `.boxed()` | `rtc-interceptor-0.20.0/src/registry.rs:78,85,177` |
| ICE over TCP (host/passive candidates) | `PeerConnectionBuilder::with_tcp_addrs(...)`; `TransportProtocol::TCP` in `tcp_transport.rs` | `peer_connection/mod.rs:308`, `tcp_transport.rs:202` |

**`PeerConnectionEventHandler` callbacks (all `async`, default no-op):**
`on_negotiation_needed`, `on_ice_candidate`, `on_ice_candidate_error`,
`on_signaling_state_change`, `on_ice_connection_state_change`,
`on_ice_gathering_state_change`, `on_connection_state_change`, `on_data_channel`,
`on_track` (`peer_connection/mod.rs:142-169`).

**State enums (re-exported from `rtc::peer_connection::state`):**
`RTCPeerConnectionState{Unspecified,New,Connecting,Connected,Disconnected,Failed,Closed}`,
`RTCIceConnectionState{Unspecified,New,Checking,Connected,Completed,Disconnected,Failed,Closed}`,
`RTCIceGatheringState{Unspecified,New,Gathering,Complete}`,
`RTCSignalingState{Stable,HaveLocalOffer,HaveRemoteOffer,HaveLocalPranswer,HaveRemotePranswer,Closed}`,
`RTCDataChannelState{Unspecified,Connecting,Open,Closing,Closed}`.

---

## 3. Capability matrix (config → support level)

Legend: **supported** (works in v0.20, evidence cited) · **unsupported** (absent
from v0.20) · **deprecated** (present but slated for removal / replaced) ·
**blocked** (absent upstream; production cutover must wait for upstream/fork or
drop the requirement).

| Capability (current config) | 0.14 mechanism | v0.20.0 status | Level | Evidence |
|---|---|---|---|---|
| **Shared ICE UDP mux** (`ice_udp_mux_port`) | `SettingEngine::set_udp_network(UDPNetwork::Muxed(mux))` — one socket shared by all PCs | **Not exposed.** `set_udp_network` / `UDPNetwork` / `SetICETCPMux` are present but **commented out**; no mux injection point exists. Each PC binds its own `with_udp_addrs` sockets. | **blocked** | `setting_engine.rs:542-548` (commented), `setting_engine.rs:975` (`SetICETCPMux` commented) |
| **Fixed UDP port** | (via mux or ephemeral range) | Upstream can express a single per-PC port via `with_udp_addrs`, but Actr cannot atomically reserve/configure it across multiple PCs without the pre-bound-socket seam. Actr therefore exposes ephemeral per-PC ports only and rejects fixed-port config. | **unsupported in Actr / fail-fast** | `peer_connection/mod.rs:303`; `crates/actr-config/src/config.rs` |
| **UDP port range** (`udp_ports`) | `SettingEngine::set_udp_network(UDPNetwork::Ephemeral(EphemeralUDP::new(min,max)))` | **Not exposed** (same commented-out `set_udp_network`). v0.20 binds explicit sockets; a range must be implemented by Actr choosing/binding a port itself. | **blocked** (needs Actr-side re-implementation) | `setting_engine.rs:542-548` (commented) |
| **NAT 1:1** (`public_ips`) | `set_nat_1to1_ips` | The setter exists, but the v0.20 gather path does not consume the stored mapping. Actr rejects `public_ips` rather than advertising private candidates under a misleading configuration. | **unsupported in Actr / fail-fast** | `setting_engine.rs:673`; `crates/actr-config/src/config.rs` |
| **TURN/UDP** | `RTCIceServer { urls: ["turn:…?transport=udp"], username, credential }` | Supported. `RTCIceServer { urls, username, credential }`; `turn_relayer` drives `TransportProtocol::UDP`. | supported | `turn_relayer.rs:298,666,803`; `server.rs` fields |
| **TURN/TCP** (`turn:…?transport=tcp`) | TURN over TCP | **Unsupported.** Relayer logs `Skipping unsupported non-UDP TURN url` and skips it (does **not** error). Tracking issue #848 **open**. | **blocked** | `turn_relayer.rs:261`; issue #848 |
| **TURN/TLS** (`turns:…`) | TURN over TLS | **Unsupported.** Relayer logs `Skipping unsupported secure TURN url` and skips it. | **blocked** | `turn_relayer.rs:256`; issue #848 |
| **IPv4 / IPv6** | `set_network_types` | Production uses dual-stack wildcard bind inputs and webrtc-rs expands them into live host candidates. Concrete `LocalAddressProvider` addresses remain available for deterministic tests and explicit platform adapters. | supported | `setting_engine.rs:610`; `address.rs`; `factory.rs` |
| **Host-candidate interface enumeration** | ICE agent auto-gathers host candidates on **all** local interfaces | Production v0.20 uses dual-stack wildcard bind inputs (`0.0.0.0` and `[::]`); webrtc-rs resolves the live interface set at the initial bind and each ICE-restart rebind. Concrete `LocalAddressProvider` inputs remain available for deterministic tests and explicit platform adapters. Mobile network path events and desktop peer-state callbacks drive recovery; a failed restart can still escalate to Hard Recovery. | supported | `factory.rs`; `address.rs`; `network_event.rs`; `recovery.rs` |
| **VNet / fault-injection test net** (`set_vnet`, test-utils) | `SettingEngine::set_vnet(Net)` | v0.20 has no VNet and `runtime-mock` supplies virtual time only. Actr uses T1 manual policy time, T2 `RuntimeFaultProbe`/deterministic signaling, and T3 real system/mobile network harnesses instead of pretending the mock Runtime is a network. | **upstream absent; replaced test strategy** | `fault.rs`; `policy_clock.rs`; `docs/webrtc-v020-test-contract.md` |
| **DataChannel send buffer limit** | (none in 0.14) | Supported (new). `with_data_channel_send_buffer_limit` + `try_send`/`writable`/`outstanding_bytes`. | supported | `peer_connection/mod.rs:402` |
| **SCTP receive window** | (fixed) | Supported. `with_sctp_receive_buffer_size`, default 1 MiB. | supported | `peer_connection/mod.rs:259` |
| **Runtime (Tokio)** | (implicit) | Supported. `runtime-tokio` → `TokioRuntime`. | supported | `runtime/mod.rs:426` |
| **Runtime (mock/virtual clock)** | — | Partial. `runtime-mock` gives virtual **timers**, not a virtual network. Disconnect/loss/recovery tests still need a custom socket backend or e2e environment. | supported (clock) / **blocked** (network) | `runtime/mock.rs` |

---

## 4. Cutover decisions and remaining external limitations

The cutover resolves unavailable capabilities through explicit removal and
startup rejection; none may silently downgrade to ephemeral/UDP behavior.

The iOS `NWPath` adapter exposes Wi-Fi, cellular, Ethernet, and other/loopback
interfaces, but Network.framework has no dedicated VPN interface type. The
Swift adapter therefore reports `vpn: false` and documents VPN detection as a
platform limitation; `.other` is not treated as VPN.

1. **Shared ICE UDP mux (`ice_udp_mux_port`) — BLOCKED.** v0.20 exposes no mux
   injection. Per RFC §"Shared UDP mux and fixed ports" and §"Non-goals", the
   missing capability MUST NOT be silently converted to a warning + random port.
   The shipped decision is to reject the legacy config at parse/startup. A
   future upstream/fork pre-bound-socket API may reintroduce it deliberately.
2. **TURN over TCP/TLS — BLOCKED (upstream issue #848, open).** The relayer
   *silently skips* `turn:…?transport=tcp` and `turns:…` URLs (warning only).
   Relay-only deployments behind UDP-blocking firewalls have no working path.
   Actr MUST NOT treat these as supported; see fail-fast policy (§5).
3. **UDP port range (`udp_ports`) — rejected.** No `EphemeralUDP` or atomic
   pre-bound-socket equivalent exists. Both complete and partial range config
   fail before node startup.
4. **VNet — replaced, not emulated.** Deterministic policy/protocol tests use
   the T1/T2 seams; actual interface/NAT/link behavior remains a T3 system or
   Android/iOS test responsibility.
5. **Host-candidate source change — implemented.** Production uses dual-stack
   wildcard binds, and webrtc-rs re-enumerates live interfaces on each
   ICE-restart rebind. Concrete providers remain test/platform seams. Mobile
   native path events and desktop PeerConnection state callbacks drive recovery;
   a failed restart can still escalate to Hard Recovery.

---

## 5. Fail-fast policy for unsupported configuration

Per RFC §"Configuration compatibility", a configured-but-unsatisfiable capability
MUST fail fast at startup or before `PeerConnection` build — never silently
degrade. Concretely for the v0.20 backend:

- **TURN transports** — parse each ICE-server URL. Reject `turn:…?transport=tcp`
  and `turns:…` with `UnsupportedRtcCapability` **before** building, because
  webrtc-rs would otherwise *skip them with only a log warning* (the
  `Skipping unsupported … TURN url` branches), silently dropping relay
  candidates. TURN/UDP passes.
- **Shared UDP mux / port range** — if `ice_udp_mux_port` or `udp_ports` is set
  while the v0.20 backend is active, return a configuration error at startup
  until the corresponding blocker (§4) is resolved or the config is deprecated.
- **Bind addresses** — an unsatisfiable bind surfaces as a build error from
  `PeerConnectionBuilder::build` (it binds the sockets up front); the factory
  propagates it rather than falling back to a random port.
- Distinguish **"not supported"** (config error, fail-fast) from **"temporarily
  unreachable"** (retry/recovery path) — they must not share a code path.

These map to the RFC error model's `UnsupportedRtcCapability` variant.

---

## 6. RFC assumptions vs. real API — discrepancies recorded

The RFC is a design document; where it diverged from the real crate, the crate
wins and the scaffolding was adapted accordingly:

1. **`PeerDriverEvent::DriverExited` cannot be sourced from the v0.20 handler.**
   The `PeerConnectionEventHandler` trait has **no** driver-exit callback; driver
   termination only surfaces as a terminal connection-state change
   (`Closed`/`Failed`). The variant is declared for forward compatibility but is
   not emitted directly by the handler; Runtime/Session supervision supplies
   the higher-level driver-failure test seam.
2. **`RTCDataChannelInit` field shapes differ from 0.14.** 0.14 used
   `ordered: Option<bool>`, `protocol: Option<String>`; v0.20 uses
   `ordered: bool` (default `true`), `protocol: String`, `max_retransmits:
   Option<u16>`, `negotiated: Option<u16>`. Stage 4 lane creation must map the
   pinned 0.14 QoS contract onto these fields in `peer/lane.rs`.
3. **No graceful per-channel `OnClose` on `close()`.** `close()` sets `closing`
   synchronously and aborts the driver; it does not run a graceful per-channel
   SCTP shutdown, so `DataChannel::poll()` may never observe `OnClose`. The
   deterministic post-close signal is `send`/`writable` returning
   `ErrDataChannelClosed`. (Validated by `v020_offer_answer_bidirectional_close_loop`.)
4. **Default send-buffer limit is unbounded.** Back-pressure is opt-in
   (`with_data_channel_send_buffer_limit`); without it `send` never blocks and
   `try_send` behaves like `send`. Actr must set an explicit limit to get the
   RFC's memory-budget behaviour.
5. **Reactor-pool size is process-global and first-wins.** Only the first
   `with_reactor_pool_size` value takes effect; Actr must treat it as
   process-level config, not per-instance (matches RFC §"Runtime strategy").
6. **0.14 `set_vnet` test hook is gone.** See blocker §4.4.
