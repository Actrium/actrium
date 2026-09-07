# Actrix Realm Infrastructure

Actrix is the single-node Realm infrastructure for the Actr network. The v1 profile carries exactly one Realm per instance,
combining Authority/AIS, Signaling (Rendezvous), STUN/TURN (Relay),
Publisher Registry, Signer, and the management plane in a single process, with SQLite as the sole persistence dependency.

This version is a production security baseline, not a security certification. Before a deployment is called
production-ready, it must complete the interoperability, abuse, recovery, and independent security review gates
listed in the [Threat Model](docs/THREAT_MODEL_REALM_V1.zh.md).

## v1 Contract

- Exactly one Realm per instance; the system administrator is the sole Realm management root for this instance.
- Single-node, SQLite-first; no HA, federation, or cross-Realm migration is promised for the first version.
- Authority issues Actor instance identities; Signaling only does rendezvous, it is not a trust root.
- Actor ID carries Realm, instance serial number, and Actor type; program identity is specified as
  `publisher/program@version`.
- Publisher ID is a canonical HTTPS domain; the domain is for discovery, Ed25519 key and digest are for long-term
  verification, the download origin is not a trust root.
- TURN credential binds Realm, official provider, endpoint audience, Actor, and expiry time.
- Native WebSocket passes identity via upgrade headers; browsers use a constrained WebSocket
  subprotocol transport. Credentials must not appear in URLs.
- Configuration and SQLite schema fail closed; legacy schema only upgrades via explicit offline script.

Full design documents:

- [Realm v1 Architecture](docs/REALM_V1_ARCHITECTURE.zh.md)
- [Threat Model and Release Gates](docs/THREAT_MODEL_REALM_V1.zh.md)
- [TURN/STUN Interop and Abuse Testing](docs/TURN_STUN_INTEROP_TESTING.zh.md)
- [Realm v1 Acceptance Evidence](docs/REALM_V1_ACCEPTANCE_EVIDENCE.zh.md)
- [Dependency Risk Register](docs/DEPENDENCY_RISK_REGISTER.zh.md)
- [Independent Security Review Package](docs/INDEPENDENT_SECURITY_REVIEW.zh.md)
- [Current API Contract](docs/API.md)
- [Signer Trust Boundary](docs/KS_GUIDE.md)

## Services

| Plane | Component | v1 Responsibility |
|---|---|---|
| Authority | AIS, Signer | Actor ID, short-lived credential, Realm signing |
| Rendezvous | Signaling | Discovery, presence, signaling relay for authenticated Actors |
| Relay | STUN/TURN | NAT probing and credential-constrained relay |
| Distribution | Publisher Registry | Publisher domain discovery, key, package metadata |
| Management | Admin HTTP/UI, gRPC | Single Realm provisioning, configuration, audit, and operations |

These components can use different domain names and ports, but the recommended v1 deployment is a single Actrix process.
Even within the same process, credential issuer/provider/audience are explicitly bound to their respective boundaries.

## Build

Requires Rust 1.95+, SQLite 3, and OpenSSL:

```bash
git clone https://github.com/Actrium/actr.git
cd actr/actrix
cargo build --release
```

Enable OpenTelemetry:

```bash
cargo build --release --features opentelemetry
```

## Configuration and Startup

The example config intentionally retains unusable secret placeholders; validation must fail without replacement:

```bash
cp config.example.toml config.toml
```

At minimum you need to set:

- `actrix_shared_key`: an independently generated inter-service PSK;
- `turn.turn_secret`: a stable per-Realm/per-provider secret;
- `services.ais.server.renewal_token_secret`: a stable renewal secret;
- `services.ais.server.worker_id`: an explicit 0..31 single-node worker ID;
- `services.signer.kek_file`, `kek_env`, or `kek`: production Signer KEK source;
- `control.admin_ui.password_hash`: Argon2id PHC hash, never a plaintext password;
- `bind.http.cert`, `bind.http.key`: production HTTPS certificate;
- `bind.ice.advertised_ip`: an externally reachable ICE address.

It is recommended to inject KEK and secrets via files or a secret manager; config file permissions should be at least `0600`.
All secrets must be independent and must not be reused.

```bash
# Validate
./target/release/actrix test config.toml

# Start
./target/release/actrix --config config.toml
```

`enable = 31` is the v1 all-in-one profile. The `prod` environment requires HTTPS, Argon2id admin hash,
non-placeholder secrets, Signer KEK, and other security conditions, otherwise it refuses to start.

## Initial Provisioning

A new database creates schema v1 on first startup, but there is still no active Realm. The system administrator
creates the unique Realm via the Admin UI or a protected Admin API, then:

1. `GET /.well-known/actr/realm.json` publishes the Realm descriptor;
2. AIS can then register Actors for trusted hosts holding the Realm registration capability;
3. Guest code never holds the Realm secret or that capability;
4. Publishers first publish `/.well-known/actr/publisher.json`, then an administrator triggers domain verification.

The online API does not provide Realm hard deletion or replacement. Realm rebuild, schema upgrade, and
incompatible data handling are all offline operations.

## SQLite Upgrade

Actrix does not guess or silently rewrite legacy databases in the main process. Stop the service first and run
the standalone script:

```bash
python3 scripts/migrate_schema_v1.py /var/lib/actrix/actrix.db
```

The script creates a backup, migrates within a transaction, and verifies `PRAGMA user_version`. Legacy Publisher
data that cannot be safely mapped requires explicitly passing `--drop-publisher-data`; multi-Realm data will be
rejected, and the administrator must first decide which Realm to keep.

## Validation

```bash
cargo fmt --all --check
cargo test --workspace -- --test-threads=1
python3 -m unittest discover -s scripts/tests -p 'test_*.py'

# UDP protocol interop and bounded malformed traffic regression
cargo test -p actrix-turn --test udp_interop -- --test-threads=1
cargo test -p actrix-stun -- --test-threads=1

cd crates/actrixd/admin/web
pnpm install --frozen-lockfile
pnpm build
```

The Web Service Worker is in a separate Cargo workspace:

```bash
cargo check \
  --manifest-path ../bindings/web/crates/sw-host/Cargo.toml \
  --target wasm32-unknown-unknown
```

## Explicitly Out of v1 Scope

- Multiple Realms per instance;
- Multi-node HA, automatic worker lease, shared SQLite;
- Third-party provider coordination or Realm federation;
- Workspace lifecycle, user-held Workspace encryption keys;
- Actro's Tab/Service URL, recovery, and multi-window product model.

The latter two belong to the Actro/Service host layer and should not be dictated by Actrix's Realm infrastructure.

## Release Gates

The current code implements fail-closed configuration, credential scoping, log redaction, rate/size limits,
key rotation basics, SQLite schema gate, UDP allocation/relay testing, bounded malformed traffic regression,
and a relay-only Chromium DataChannel interop fixture. However, a formal production declaration still requires:

- An interoperability matrix covering real browsers beyond Chrome/Chromium, native clients, mainstream
  TURN/STUN implementations, and target NAT/mobile networks;
- Sustained credential replay, TURN abuse, connection flood, and resource exhaustion stress testing;
- SQLite backup, recovery, disk corruption, and node rebuild drills;
- Key rotation/revocation drills and audit record checks;
- At least one independent security review or penetration test, with all P0/P1 findings closed.

## License

Apache License 2.0.
