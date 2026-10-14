# Direct WebSocket transport security

Peer direct WebSocket is optional: omitting `websocket.listen_port` leaves the
listener disabled. Outbound direct connections require `wss://` by default.
The listener defaults to `127.0.0.1` and requires a TLS certificate and key.
Signaling's separately configured `ws://` / `wss://` policy is unchanged.

Configure the flat `[websocket]` section in `actr.toml`:

```toml
[websocket]
listen_port = 9443
bind_address = "0.0.0.0"
advertised_host = "peer.example.com"
tls_cert = "certs/peer-chain.pem"
tls_key = "certs/peer-key.pem"
ca_cert = "certs/realm-ca.pem" # optional, for outbound direct connections
```

Paths are relative to `actr.toml`; absolute paths are also accepted. The certificate
file is a PEM chain, leaf first; the private key is unencrypted PEM. Protect the
key with filesystem permissions. Provision a certificate whose SAN matches the
advertised DNS name or IP. `bind_address` is a literal IP independent of the
advertised hostname. Wildcard binds require an explicit reachable advertised host,
and an advertised host that is not loopback is rejected while the bind stays on
its loopback default — set `bind_address` to the interface that serves it.
IPv6 hosts may be bracketed or unbracketed. A zero `listen_port` binds an
ephemeral port and advertises the actual bound port. A startup warning is logged
when a configured listener is reachable only from loopback.

The configured CA bundle **replaces** native/public roots for direct peers. Without
it, clients use native and WebPKI roots. Clients validate the certificate chain,
validity, and endpoint name; there is no verification bypass. Client-only nodes
can set `ca_cert` without a listener or server key. TLS configuration is validated
before registration; incomplete, unreadable, or invalid material fails closed.
TLS listeners advertise `wss://` and do not accept plaintext upgrades. Inbound
TLS/upgrade handshakes have a ten-second deadline and at most 64 concurrent slots.

For explicitly opted-in local development only:

```toml
[websocket]
listen_port = 9001
allow_insecure = true
# bind_address defaults to "127.0.0.1"
```

This permits plaintext only for loopback listeners and literal loopback destination
IPs (including `::1`). It does not permit `ws://localhost`, other DNS names, or
non-loopback listeners/destinations. It never falls back from failed TLS to
plaintext. Loopback plaintext is not protection from other local processes.

## Threat model and remaining authentication work

TLS protects direct RPC/DataStream traffic and the existing `X-Actr-Credential`
from network observation/modification when endpoint certificates and CA trust are
correctly provisioned. HTTP upgrade errors and connection Debug output omit
credential-bearing details in this transport's own errors and Debug output.
Tungstenite dependency trace logging can still emit the raw HTTP upgrade request;
keep `tungstenite` below trace level and protect logs. This change does not claim
to sanitize arbitrary dependency-level wire logging.

The wire protocol still sends the reusable AIS-signed credential. Its signature,
expiry, and source Actor match are checked by the existing gate. It is **not** a
freshness proof: a recipient or anyone who obtains the credential can replay it
until expiry. The TLS certificate authenticates an endpoint name, not the expected
destination Actor ID. IdentityClaims does not bind an Actor to a possession key.
A trusted or misdirected endpoint can therefore collect credentials; CA trust and
discovery integrity remain part of the deployment trust boundary. No Actor mutual
authentication, destination binding, or replay resistance is claimed here.
The existing application secure-channel state is not promoted by TLS.

Issue #81 remains open for a versioned authentication design: bind an Actor
possession key to AIS identity and a fresh source/destination/session transcript,
or issue destination-bound short-lived single-use tickets with atomic replay
checking. Include concurrent replay and wrong-destination tests in that work.

## Migration and rotation

Existing plaintext non-loopback configurations now fail closed. Deploy endpoint
certificates and outbound CA trust before enabling exposed direct listeners; set
`bind_address` explicitly. The existing discovery `ws_address` and credential
headers remain wire-compatible. Older clients capable of WSS can connect if they
trust the server certificate; new default clients reject older plaintext direct
endpoints. Private-CA trust must be provisioned on every client. There is no new
protocol negotiation or general mixed-version authentication guarantee.

For CA rotation, distribute a bundle containing old and new CAs, restart clients,
replace server chains/keys and restart servers, then remove the old CA and restart
clients. Configuration and certificates are loaded at startup, not hot-reloaded.
Keep old valid TLS material for rollback; disable the optional direct listener if
TLS cannot be provisioned instead of exposing plaintext. Other transport behavior
and signaling configuration remain separate.

Direct WebSocket TLS builds on rustls with the aws-lc-rs provider. Building the
e2e and WinUI workload lockfiles therefore compiles `aws-lc-sys`, which needs
CMake (and NASM on Windows MSVC) — the CI images already provide both.
