# actrixd service composition

`ServiceManager` is the composition root for Realm v1:

- Shared HTTP/HTTPS listener: Realm descriptor, Admin, Issuer, Signaling,
  Publisher Registry, Signer gRPC;
- ICE listener: STUN/TURN;
- Single SQLite schema v1;
- Router/certificate reload with bounded shutdown.

Public names are uniformly Signer and Publisher Registry. Internal file names `publisher.rs`, crate
`actrix-publisher-registry` are historical implementation names only and should not appear in new configuration,
JSON, or documentation.

See architecture and lifecycle in
[`../../../../docs/ARCHITECTURE.md`](../../../../docs/ARCHITECTURE.md) and
[`../../../../docs/SERVICES.md`](../../../../docs/SERVICES.md).
