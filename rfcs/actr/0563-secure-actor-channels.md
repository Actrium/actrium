# RFC-0563: Secure Actor Channels

- Status: Proposed
- Date: 2026-08-13
- RFC PR: (to be added on submission)
- Tracking issue: [Actrium/actrium#81](https://github.com/Actrium/actrium/issues/81) (historical [Actrium/actr#563](https://github.com/Actrium/actr/issues/563))
- Superseded by: None
- Related: [Actrium/actrium#81 — encrypt peer direct WebSocket transport and
  prevent AId credential replay](https://github.com/Actrium/actrium/issues/81)
  (historical [Actrium/actr#349](https://github.com/Actrium/actr/issues/349)),
  [RFC-0504 — Actr RTC / webrtc-rs architecture](./0504-webrtc-v020-rtc-controller-architecture.md),
  [Actrium/actrium#306 — direct WebSocket trust](https://github.com/Actrium/actrium/issues/306),
  [RFC-0400 — event-driven connection lifecycle and
  recovery](./0400-event-driven-connection-lifecycle-and-recovery.md)

## Summary

Introduce `SecureActorChannel`, a transport-independent abstraction that sits
above the existing `WireHandle`/`DataLane` layer and mutually authenticates both
Actor endpoints of every Actor-to-Actor channel. Both ActorIds, the session or
exchange identifier, fresh nonces, and the transport's session key material
(DTLS fingerprint on WebRTC, ephemeral DH keys on WebSocket) are bound into a
single authenticated transcript, signed and verified locally by each Actor under
its peer's `AIdCredential`. No application payload may be sent or accepted before
mutual authentication and key confirmation complete; verification failure fails
closed. The abstraction is identical over WebRTC, WebSocket, TURN-relayed, and
future transports; signaling, relays, and proxies become untrusted for content
confidentiality and Actor impersonation.

## Motivation

The only existing enforcement that an Actor credential binds to the relay source
runs **on the signaling server**, not at either Actor endpoint. In
`crates/loran-signaling/src/server.rs`, `handle_actr_relay`
(`:1291-1405`) clones `relay.source` at `:1299`, validates the credential via
`AIdCredentialValidator::check` at `:1361-1367`, and performs the binding check
`claims.actor_id == relay.source` at `:1384-1397`, rejecting a mismatch. The
credential is then forwarded to the target at `:1431,1451`.

The receiving Actor trusts this. In `crates/actr-hyper/src/wire/webrtc/coordinator.rs`,
the relay/source dispatch at `:4346-4494` decides local-versus-remote handling by
comparing `relay.source == local_id` and otherwise consumes `relay.source`
directly as the peer identity, with no local credential re-verification. The web
runtime does the same at `packages/actr-web/crates/sw-host/src/runtime.rs:1392,1924,
1959,2126-2128`. The trust model therefore reduces to "the signaling server did
not substitute peer identity, SDP, or DTLS fingerprint" — exactly the assumption
this RFC removes.

Two concrete gaps follow:

1. **WebRTC has no Actor-level identity binding.** webrtc-rs performs DTLS
   internally; the Actr layer never reads or binds the DTLS fingerprint. The
   closest existing code is SDP/ICE-ufrag extraction at
   `crates/actr-hyper/src/wire/webrtc/coordinator.rs:271-360` and the two-stage
   establishment prose at `:161-225`. DTLS encrypts between DTLS endpoints but
   proves nothing about which Actors those endpoints are.

2. **The direct WebSocket path exposes replayable credential material in
   plaintext headers** (`crates/actr-hyper/src/wire/websocket/connection.rs:171-197`
   sets `X-Actr-Source-ID` and conditionally `X-Actr-Credential`), as called out
   by #349. The inbound gate verifies the credential signature
   (`crates/actr-hyper/src/wire/websocket/gate.rs:319-374`) but only authenticates the
   credential itself, not "this credential belongs to *this* session."

Scenarios the current design cannot prevent:

- A compromised or malicious signaling server substitutes the peer `ActorId`,
  the SDP, or the DTLS fingerprint, causing an Actor to attribute traffic to the
  wrong peer.
- Replay of a previously valid handshake or header credential across sessions.
- Downgrade to `ws://` plaintext or to an unauthenticated fallback when a secure
  path is expected.

The wire format and existing contracts do not prevent this change: signaling
already carries `ActrRelay { source; credential; target }`
(`crates/actr-protocol/proto/signaling.proto:94-98`), and the `WireHandle` abstraction
(`crates/actr-hyper/src/transport/wire_handle.rs:31-72`) already unifies WebRTC and
WebSocket under one trait, so a `SecureActorChannel` wrapper composes without
touching transport internals.

## Detailed design

### 1. Channel state machine

`SecureActorChannel` wraps a `WireHandle` (`crates/actr-hyper/src/transport/wire_handle.rs:38-75`)
and exposes the same `DataLane` surface (`crates/actr-hyper/src/transport/lane.rs:274-345`)
to upper layers. Its state machine is:

```
New -> Handshaking -> Verified -> Usable -> (Rekeying -> Verified)* -> Closed | Failed
```

Application `send`/`recv` gates on `Usable`. Today the "usable" transition is pure
transport readiness: DataChannel `on_open` emits `DataChannelOpened`
(`crates/actr-hyper/src/wire/webrtc/connection.rs:817-831`), and a WebSocket enters Ready
directly via `from_server_stream`
(`crates/actr-hyper/src/wire/websocket/connection.rs:141-156`). This RFC requires that
`WireStatus::Ready` (`crates/actr-hyper/src/transport/wire_handle.rs:77-88`) does not
surface to upper layers until `Verified` holds. Concretely, the pool-level ready
set (`crates/actr-hyper/src/transport/wire_pool.rs:238-268,311-370,374-491`) gains a
secure-channel precondition: a wire becomes a candidate for `Ready` only after
its `SecureActorChannel` reports `Verified`.

`Failed` is terminal and fails closed — no plaintext or unauthenticated
downgrade is offered. The recovery machinery in RFC-0400 drives reconnection;
this RFC specifies only that a reconnected wire re-enters `Handshaking`.

### 2. The authenticated transcript

Every handshake builds a canonical, ordered, length-prefixed byte buffer over
these fields and feeds its hash into key derivation:

- protocol version (uint32);
- source `ActrId` (`crates/actr-protocol/proto/actr.proto:11,27-32`: realm + serial + type);
- target `ActrId`;
- session or exchange identifier (a fresh `u64` minted per handshake by the
  initiator, carried in signaling alongside the existing `ActrRelay`);
- initiator and responder nonces (32 bytes each, fresh per handshake);
- session key material — the DTLS fingerprint for WebRTC (see §8), the ephemeral
  DH public key for WebSocket;
- negotiated security parameters (transport kind, cipher-suite id, KDF label).

The canonical encoding is fixed-order, length-prefixed UTF-8/protobuf fields so
that two independent implementations produce byte-identical transcripts. The
transcript hash (BLAKE3) is what signatures and the KDF consume.

### 3. Handshake protocol — recommended: a SIGMA-derived pattern

Actr already issues per-Actor Ed25519 credentials through a trusted issuer (Issuer,
`crates/loran-signaling/`), and `IdentityClaims`/`AIdCredential` already carry
`realm`, `actor_id`, `actor_type`, and `expires_at` (`crates/actr-protocol/proto/actr.proto:64-90`).
A SIGMA-style mutually-authenticated Diffie-Hellman exchange composes cleanly:

- DH ephemerals provide forward secrecy.
- Each Actor signs the transcript hash with its Ed25519 signing key, binding both
  identities and the session material.
- MAC-then-verify ordering provides identity protection: a transcript's
  signatures are not exposed until the peer has confirmed the session key, so a
  passive observer cannot link traffic to Actor identities.

The WebRTC path does **not** run a second key exchange. The DTLS session is
already established by webrtc-rs; the Actors instead sign a `PeerBinding`
assertion over `{ both ActorIds, exchange id, nonces, DTLS fingerprint }` and
verify it locally. The WebSocket path runs the full SIGMA DH handshake over the
untrusted byte stream.

This RFC recommends SIGMA; the principal open question for review is whether to
adopt Noise (e.g. `Noise_XX` or `Noise_XKpsk0`) instead — see Alternatives.

### 4. Credential and trust-root verification

No new cryptographic primitive is introduced. The design reuses:

- `AIdCredentialValidator` (`crates/loran-platform/src/aid/credential/validator.rs:17-117`)
  and the pure-function `AIdCredentialVerifier`
  (`crates/loran-platform/src/aid/credential/verifier.rs:13-78`) for Ed25519
  signature verification and claims decoding;
- `IssuerKeyCache` (`crates/actr-hyper/src/key_cache.rs:1-99`), already wired into the
  WebSocket gate (`crates/actr-hyper/src/wire/websocket/gate.rs:319-374`), to resolve
  Issuer signing keys on the WebRTC side too.

The peer-signed transcript is verified with the peer's `AIdCredential`, and
`claims.actor_id` must equal the asserted peer `ActorId`. This moves the binding
check that today lives only at `crates/loran-signaling/src/server.rs:1384-1397`
to **both endpoints locally**. The signaling server may continue its check as
defense-in-depth, but receivers no longer depend on it.

### 5. Key derivation and confirmation

HKDF (BLAKE3-based) over the transcript hash, with `info = protocol_version |
source ActorId | target ActorId | exchange id | transport kind`. Key confirmation
is a MAC exchange: each endpoint MACs a fixed confirmation string under the
derived key and sends it; both sides verify before entering `Usable`. No
application payload is sent or accepted until both MACs verify — this is the gate
that §1 enforces.

### 6. Replay and downgrade protection

- **Replay:** initiator and responder nonces are fresh random per handshake and
  are part of the signed transcript, so a captured handshake replayed in a
  different session fails the transcript-hash check and the key-confirmation MAC.
  The exchange id is also fresh per handshake, so replay within a different
  exchange is detected even if nonces collided.
- **Downgrade:** protocol-version and cipher-suite negotiation are part of the
  authenticated transcript. A stripped or forced fallback to plaintext or an
  unauthenticated transport fails closed because `Usable` is never reached. `ws://`
  is permitted only as an untrusted ciphertext carrier **after** a secure session
  is established; the plaintext `X-Actr-Source-ID`/`X-Actr-Credential` headers at
  `crates/actr-hyper/src/wire/websocket/connection.rs:171-197` are removed.

### 7. Reconnection, renegotiation, rekey, and ICE restart

- **Reconnection** mints a new exchange id and fresh nonces; the secure session
  rebinds from `Handshaking`. RFC-0400's recovery supervisor drives the wire
  reconnection; this RFC requires the secure channel to re-handshake before
  `Usable` resumes.
- **ICE restart** (`crates/actr-hyper/src/wire/webrtc/coordinator.rs:70-99,699-790`)
  preserves the Actor identity binding as long as the DTLS fingerprint is
  unchanged. If DTLS renegotiates a new fingerprint, the `PeerBinding` must be
  re-signed and re-verified before `Usable` is re-entered.
- **Rekey** rotates session keys without re-authenticating identities, via a
  rekey sub-protocol keyed off the current transcript hash. The state passes
  through `Rekeying -> Verified` without a full handshake.
- **WebRTC renegotiation** of data channels does not affect the secure channel
  state as long as the DTLS session and fingerprint are unchanged.

### 8. Transport-specific mappings

**WebRTC.** Native DTLS stays; webrtc-rs continues to own the encrypted transport.
A new `PeerBinding` message, signed by each Actor over `{ source ActorId, target
ActorId, exchange id, nonces, local DTLS fingerprint }`, is relayed through
signaling alongside `ActrRelay`. The receiver verifies it locally using
`IssuerKeyCache` and the peer `AIdCredential`, and rejects any mismatch. Signaling
still relays the message but cannot forge it (it holds no Actor signing key).
This requires exposing the local DTLS fingerprint to the Actr layer — currently
not surfaced; the `PeerConnection` wrapper at
`crates/actr-hyper/src/wire/webrtc/adapter.rs:334-596` must grow an accessor.

**WebSocket.** WebSocket is treated as an untrusted byte transport. After TCP/TLS
connect, the SIGMA DH handshake runs over the stream; only after `Verified` may
application frames flow. The plaintext `X-Actr-Source-ID`/`X-Actr-Credential`
headers (`crates/actr-hyper/src/wire/websocket/connection.rs:171-197`) and the inbound
header parse path (`crates/actr-hyper/src/wire/websocket/server.rs:8-237`) are removed.
WSS remains the recommended production default for outer metadata and handshake
protection, but Actor message security must not depend on WSS being present or on
where TLS terminates — this is the reconciliation with #349.

### 9. Required security test vectors (negative)

Derived from the #563 acceptance criteria, each must be an executable test that
fails closed:

- Malicious signaling substitution of `source`, `target`, session id, or DTLS
  fingerprint -> handshake fails.
- Replaying a valid handshake from another session -> handshake fails.
- A signaling server, TURN relay, or transport proxy cannot derive Actor message
  keys.
- No Actor payload is sent or accepted before mutual authentication and key
  confirmation.
- WebRTC and WebSocket expose the same secure-channel state to upper layers.
- No plaintext or unauthenticated fallback is possible.
- Reconnection, renegotiation, rekeying, and ICE restart preserve or
  re-establish the complete identity/session binding.
- Wrong target ActorId, expired credentials, and failed key confirmation each
  fail the handshake.

## Drawbacks

- Added handshake latency and complexity on every Actor channel; the WebRTC path
  gains a signed-binding round trip even though DTLS already encrypts.
- The WebRTC path requires adapter changes to expose the DTLS fingerprint to the
  Actr layer (`crates/actr-hyper/src/wire/webrtc/adapter.rs:334-596`).
- The signaling wire format changes (new `PeerBinding` message in
  `crates/actr-protocol/proto/signaling.proto`); this uses the 0.x breaking-change
  window.

## Alternatives

- **Trust the signaling server (status quo).** Rejected — #563's premise is that
  signaling, TURN, and proxies must be untrusted for content confidentiality and
  Actor impersonation. The status quo delegates identity binding to an
  intermediary that the issue explicitly names as untrusted.
- **TLS / mTLS as the only security root.** Rejected — outer TLS does not bind
  Actor identity to the session, and where TLS terminates (a proxy, a TURN server,
  a load balancer) the content is exposed. WSS is recommended as outer
  protection only, not as the root of Actor message confidentiality.
- **Noise instead of SIGMA.** Genuinely considered and the main open question for
  review. `Noise_XX` offers a similar mutually-authenticated forward-secret
  handshake with a smaller analysis surface and existing implementations. The
  argument for SIGMA is its explicit MAC-then-verify identity-protection ordering
  and its clean composition with a pre-existing external PKI (issuer-issued
  `AIdCredential`s), which maps directly onto the Actor credential model. The
  argument for Noise is implementation maturity and a single well-studied state
  machine. This RFC recommends SIGMA but defers the final choice to review.
- **DTLS-fingerprint authentication for WebRTC only, plus a separate WebSocket
  scheme.** Rejected — it yields two divergent security models and two
  implementations, violating the transport-independent invariant. This RFC
  unifies both under `SecureActorChannel` with a shared transcript and KDF, even
  though the WebRTC mapping reuses DTLS and the WebSocket mapping runs a full DH.

The consequence of making no change is that a compromised signaling server can
impersonate any Actor to any other Actor, which is the threat #563 was opened to
close.

## Compatibility and phasing

- **Wire format:** adds a `PeerBinding` message to
  `crates/actr-protocol/proto/signaling.proto` and a secure-channel handshake
  sub-protocol. This is a breaking change to the signaling relay path; it uses
  the 0.x breaking-change window (a `feat!:` PR per the Conventional Commits
  policy in `CLAUDE.md`). The WebSocket direct path removes the
  `X-Actr-Source-ID`/`X-Actr-Credential` headers, also breaking for 0.x.
- **Public API:** upper-layer `WireHandle`/`DataLane` consumers see no change to
  the trait surface; `SecureActorChannel` wraps it. `WireStatus::Ready` semantics
  tighten (never surfaces before `Verified`), which may affect code that assumes
  transport readiness implies payload readiness.

Implementation phases, each a separate PR gated on the prior:

1. **This RFC** lands and is reviewed.
2. `SecureActorChannel` skeleton + state gate above `WireHandle`, transparent
   passthrough (no new crypto) — wires the seam and the `Verified` precondition
   in `WirePool`'s ready set.
3. WebRTC: `PeerBinding` signing + local verification + DTLS-fingerprint export
   from `crates/actr-hyper/src/wire/webrtc/adapter.rs`; receiver-side check in
   `crates/actr-hyper/src/wire/webrtc/coordinator.rs` and
   `packages/actr-web/crates/sw-host/src/runtime.rs`.
4. WebSocket: SIGMA handshake over the byte stream; remove plaintext bearer
   headers (closes #349).
5. Reconnection, rekey, and ICE-restart rebinding per §7.
6. Negative test suite per §9.

Acceptance criteria for the RFC phase: this document is reviewable, every claim
cites a concrete code path, §Detailed design covers all eight items #563's "RFC
follow-up" list names (state machine, transcript, credential verification, KDF
and confirmation, replay and downgrade, reconnection and renegotiation,
transport mappings, test vectors), and each #563 acceptance checkbox maps to a
design item or a test vector.

## Unresolved questions

- Noise vs SIGMA as the recommended handshake — the principal decision for review.
- Whether to bind TURN relay identity (the relayed candidate) into the
  transcript, or treat TURN as a pure untrusted relay.
- Credential rotation interval policy — left to a separate operational RFC.
- Communication metadata privacy (addresses, timing, traffic size) — explicitly
  out of scope per #563.
- The exact `PeerBinding` wire encoding and whether it carries over signaling
  inline or via a separate control message.

## Future possibilities

- Surface secure-channel security status (peer ActorId, verified-at, key age) to
  upper layers and telemetry, complementing the connection lifecycle events of
  RFC-0400.
- Post-quantum key exchange in the transcript without breaking the binding model
  — the transcript is versioned, so a future KEM can be added as a negotiated
  parameter.
- A single `SecureActorChannel` test harness usable by all transports, reducing
  the cost of adding future transports.
