# Loran configuration

Development, reference, and production configuration for the composed Loran
service suite belongs here.

## Development Realm admission

`[realm.dev_join]` applies to every active Realm hosted by the AFP.
It defaults to `mode = "off"` and `default_role = "editor"`.
The Loran Admin UI's **Issuer Service → Development admission** form saves
the configuration and reloads it. File edits take effect on SIGHUP.
The service needs write access to its config file, including a matching
`ReadWritePaths` entry when systemd uses `ProtectSystem=strict`.

- `off` uses one-time invitations.
- `open` accepts a signed `join` without `invite`, creating an Editor or Viewer
  as configured. Anyone who knows a hosted Realm's name can join.
- `reusable_invite` lets an unused member or client invitation admit multiple
  devices until it expires or is revoked. Owner recovery invitations remain
  single-use. When this mode is disabled, a remaining invitation becomes single-use.

Each device still proves its own key and receives a stored membership.
Retries retain the existing identity and role. Revoked keys, removed members,
inactive Realms, and membership quotas still apply. Turning the switch off
stops new development admissions. Revoke already admitted clients separately.
The signed Realm Profile's membership policy is unchanged, so treat this as
an explicit deployment override for development.

```sh
actr realm join --realm echo@realm.example --output ./echo-client.json
actr realm members --key ./echo-client.json
```

The SDK's `RealmMembershipClient.join("", label)` sends the same signed
invite-free request and retains normal endpoint and response validation.

## Standalone UDP STUN/TURN

Use the regular Loran executable and the existing configuration schema.
`ice.example.toml` enables only UDP STUN/TURN (`enable = 6`), disables both
control endpoints, and omits `[bind.http]`. In this configuration the process
starts neither an HTTP listener nor a database and needs no Issuer, signaling
service, actor identity, SQLite path, or internal service shared key.

Build the `loran-server` package and run its `loran` executable:

```sh
cargo run --release -p loran-server --bin loran -- --config loran/config/ice.example.toml
```

Copy the example, replace `bind.ice.advertised_ip` with a reachable relay
address, and configure `[turn.users]` or `turn.turn_secret`.
SIGINT and SIGTERM gracefully stop the services. SIGHUP reloads the existing
configuration and restarts ICE when its binding or credentials change;
adding an HTTP listener requires a process restart.
Open the configured UDP listener and the complete inclusive relay port range.
NAT must forward those relay ports without changing their numbers. Clients use
`stun:host:3478` and `turn:host:3478?transport=udp` (bracket IPv6 hosts).

For STUN alone use `enable = 2`; `[turn]` may be omitted. `enable = 4` starts
TURN with built-in Binding support. Both control endpoints must be disabled
when `[bind.http]` is omitted. Existing deployments with an HTTP binding keep
their control plane, database initialization, and production HTTPS validation.

TURN uses long-term credential authentication. Nonempty `[turn.users]` selects
static username/password credentials. In an independent deployment,
`turn.turn_secret` selects TURN REST credentials, even when an HTTP monitoring
listener is enabled. A deployment with persistent state and an Issuer,
Signaling service or configured authority uses Realm-scoped credentials
instead. Configure the shared secret explicitly for deployment.
Static credential strings and realms use the
RFC 8265 OpaqueString profile. SHA-256 password derivation and message integrity
are supported, with legacy MD5/SHA-1 compatibility for existing clients.
An external issuer can produce REST credentials:

```text
username = decimal_expiration_timestamp + optional(":" + opaque_suffix)
password = base64(HMAC-SHA1(shared_secret, username))
```

The suffix has no protocol-level identity meaning. Clients never receive the
shared secret. Expiration prevents new allocations; authenticated existing
allocations may refresh with their original key. Explicit session revocation
belongs to the external caller, using `Server::delete_allocations_by_username`.
Secret rotation prevents new admissions using the old secret; revoke existing
allocations separately when required by policy.

The library accepts an external `AuthHandler`, realm, bound UDP socket,
advertised relay IP and port range. Each deployment exposes one relay address
family; IPv4 is the default allocation family. IPv6 clients explicitly request
IPv6 relays. A request for both families receives a standard partial-success
response when only one is configured. TCP, TLS and DTLS are outside this profile;
the implementation limits and upstream fixes are listed in
the [upstream dependency notes](../../docs/actr/upstream-patches.md).

## Actrium integration

Existing composed deployments retain their current startup and shared-secret
configuration. `enable = 6` enables STUN/TURN; `enable = 4` also answers Binding
requests through TURN. Disabling both control endpoints and omitting
`[bind.http]` runs these services independently through the same entry point,
without HTTP or a database.
`turn.relay_port_range` is now enforced instead of ignored.

The Issuer generates Realm-, provider-, Actor- and client-key-scoped TURN
credentials for composed deployments. The TURN adapter checks the active Realm
and Actor binding and closes their allocations when authorization is revoked.
Native and browser consumers update the credentials of existing peer connection
configurations and use the newest credentials for future connections. An ICE
restart creates a new allocation rather than changing the username of an
existing allocation (which correctly returns 441).

Use shared-secret mode when the Issuer provides credentials. Static-user mode
is intended for external clients with independently provisioned credentials.
To prevent relays from reaching private services, production TURN filters
loopback, private, link-local and other prohibited peer addresses.

The RTC transport adapter keeps the original credential on each established
allocation, applies rotated credentials to future allocations, and sends
standard `Refresh(LIFETIME=0)` requests before closing or rebinding its sockets.
This also prevents old relay allocations from accumulating during recovery.
The upstream fix is documented in the
[dependency notes](../../docs/actr/upstream-patches.md).

## WSS relay

`relay.example.toml` uses the regular launcher with `enable = 32`, static
externally provisioned endpoint tokens, and both control endpoints disabled.
It needs neither Issuer nor a database/internal shared key. The example binds
loopback for development. Production uses `env = "prod"`, `[bind.http]`
`cert`/`key`, a reachable bind address, and WSS; the existing TLS listener is
shared by all HTTP services. Reverse proxies must preserve WebSocket Upgrade
and `Sec-WebSocket-Protocol` and allow at least the configured heartbeat timeout.
An upgraded relay connection must stay pinned to one instance. Endpoint pairing
is currently process-local; independent replicas need endpoint affinity or
separate namespaces, and do not form a distributed relay mesh.

Relay adds bit 32 to the existing enable mask. `enable = 38` runs relay plus
UDP STUN/TURN; `enable = 63` runs all six services. Add the ICE binding and TURN
authentication settings from `ice.example.toml` when enabling bits 2/4.
Existing masks retain their previous meaning and do not silently enable relay.
For SDK traffic, `relay.limits.max_frame_bytes` must be at least 65556 bytes;
Loran rejects smaller values when the local relay is enabled.

The trusted bootstrap origin exposes `GET /relay/config` with protocol version,
local availability and optional public WebSocket URL. `relay.public_url` can
advertise a separate host while the local relay bit is disabled. SDKs reject a
WS downgrade when bootstrapped through WSS. The endpoint accepts upgrades at
`/relay/ws` with the `loran-relay.v1` subprotocol. Authentication is in the first
text frame; no token is placed in a URL. See the [protocol and architecture
design](../../docs/actr/wss-relay-design.zh.md) for framing and readiness.

For a composed Actrium deployment, set bit 32 and `[relay] auth = "aid"`
(the default). The adapter verifies short-lived Realm Issuer-signed,
relay-scoped identity certificates and a private admission secret bound to each
certificate. It binds the canonical identity, Actor type and Realm client key
to routing endpoints, enforces expiry and checks the Realm Actor ACL before
pairing peers. It checks current Realm and Actor authorization and closes affected sessions on
revocation. Ordinary signaling AId bearers are rejected and are never sent to
the relay. The adapter uses the same Realm-scoped trusted-key cache as
signaling. The generic relay core contains no Actor identity, Issuer, discovery,
or workload logic. Static-token deployments
serve generic clients implementing the versioned subprotocol; this is not a
standard TURN endpoint.

Actrium application nodes no longer listen for peer WebSocket connections or
advertise `ws_address`. Remove the obsolete `[websocket]` section from runtime
configuration; the parser reports migration advice if it is present. Native
and browser SDKs discover relay automatically from their existing signaling
bootstrap. Applications keep the same RPC and DataChunk APIs, with no relay URL,
server certificate or listen port to configure. Relay and WebRTC connect
concurrently; verified direct ICE outranks relay, which outranks TURN/unknown
RTC for data. Native media continues to use WebRTC and TURN.

Reliable streams retain their existing healthy path until the peer acknowledges
an SDK barrier after prior chunks enter its receive queue. RPC does not wait for
this handover. Unsupported peers keep the old path; path failure retains the
existing uncertain-delivery contract without automatic replay.

The relay admission credential and certificate use the V2 identity protocol.
Coordinate the Issuer/relay and SDK rollout: older clients cannot authenticate
to the upgraded relay, and the new SDK cannot use an older relay identity issuer.
Remove the obsolete application WebSocket configuration in that deployment. The new browser SDK emits
protobuf DataChunk frames and accepts its previous length-prefixed format, but
old browser SDKs cannot receive the new format or use the relay. Mixed-version
bidirectional stream compatibility is not provided by this migration.

Native and browser SDKs establish mutually authenticated end-to-end Noise XX
sessions (X25519, ChaCha20-Poly1305, SHA-256) over the relay. The relay sees
ciphertext and routing metadata, not application message bodies or session
keys. SDKs enroll short-lived public-key certificates at the configured trusted
Issuer using both a signed Realm client proof and their private renewal proof;
production WSS deployments require HTTPS for the Issuer. Upgrade the Issuer and
Loran before the communicating SDKs. Private renewal proofs
and signaling bearers never go to relay, and no plaintext fallback is permitted.
The generic/static-token relay only forwards bytes; independent clients must
supply their own end-to-end security protocol, just as TURN clients do.

A relay failure does not replay messages whose delivery is
uncertain; RPC owners retain the existing delivery/retry semantics. Connections
and peer sessions are re-established with bounded jittered backoff. SIGHUP
closes sessions from the old configuration and clients reconnect; SIGTERM
cancels relay tasks and closes upgraded connections before shutdown.

A failed SIGHUP rebuild keeps the current router and relay sessions running;
only a successfully built replacement cancels the old sessions. Enabling the
first database-backed service on a storage-free instance requires a process
restart, even if a SQLite path was already configured. Reload rejects that
transition explicitly and leaves the existing relay available.

Native TURN/WebRTC fixes come from pinned upstream Git commits via Cargo
overrides. Independent workspaces within the repository repeat those overrides
in their root manifests. Consumers outside the repository must configure both
override tables themselves, as described in the
[dependency notes](../../docs/actr/upstream-patches.md).
