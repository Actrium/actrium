# Loran

Loran is Actrium's default and reference shore-side service suite. It provides
the service composition around Actr peers: signaling, STUN/TURN, signing,
discovery, package distribution, control, administration, and observability.

The public bootstrap roles remain separate even when one Loran process serves
them together:

- `/.well-known/actr` publishes the authority descriptor;
- `loran-packages` resolves opaque Targets and serves content-addressed `.actr`
  artifacts;
- the Publisher Registry manages publisher identity, signing authority, and publication audit;
- signaling, Issuer, and ICE remain independent runtime services.

Package distribution is configured under `[services.packages]`, not inside an
Actor package. This keeps the mapping from a stable Target to a deployed
release in the authority's operational configuration. It can later be moved
to another Loran node, an object store, or a CDN without changing the Actor.

`config/` and `deploy/` define how the suite is assembled and operated. The
main daemon is under `../crates/loran-server/`; reusable services live in the
other `../crates/loran-*` directories. Their published Rust package and binary
names still use Actrix compatibility names until the separate semantic rename
is completed.
