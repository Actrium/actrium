# Minimal Actor state profile

This profile supplies a state binding for the first WASM Service assurance
flow. A trusted Service Host supplies three measured fingerprints. Each
fingerprint is a 32-byte SHA-256 digest. The instance configuration includes
the resolved deployment settings. The effective capability fingerprint
represents the runtime grants and the effective egress policy. The workload
loader and broker own the individual measurement profiles.

## Encoding

Compute SHA-256 over a compact UTF-8 JSON array with the following ordered
strings.

1. The layout identifier `actrium.actor-state/1`
2. The canonical ActorId used by the Service session
3. The revision in decimal, with no leading zeroes
4. The lowercase hexadecimal package fingerprint
5. The lowercase hexadecimal instance configuration fingerprint
6. The lowercase hexadecimal effective capability fingerprint

For `actor-1` and fingerprints filled with bytes `01`, `02`, and `03`,
respectively, the digest is
`62707663bafa1323e0c2e5b24944e5d1d4fda8c0083c9345584d267bf1c6c4e0`
at revision 1 and
`228e44e0e1e0695025d27ddc4ed3c9bea1c25109c6b2a9a2b5f08eee0caf10ec`
at revision 2.

## Commit and publication

A managed Host starts a new runtime generation through
`LifecycleService::restart`. After loading the workload and applying its
policy, it calls `mark_running_with_actor_state` with that generation and the
three measurements. The lifecycle manager increments the last revision and
stores it with the digest in the same atomic write as the Running transition.
Stopped runs retain the last commit for the next revision. An unmeasured start
publishes no digest, and a corrupt prior commit prevents a measured start from
silently restarting the revision history.

The Host creates its session service from the returned instance using
`InMemorySessionService::new_with_actor_state`. The Service ActorId and
runtime generation must match. The signed Attach, Takeover, and Resume replies
carry the digest in the existing privacy field for that run. The session
registry retires that digest when invalidating or restarting the runtime.
Hyper retains the snapshot only after verifying the response. The future
assurance query must establish freshness against the then-current digest.

This profile covers local restart and configuration rollback under the same
ActorId. Runtime measurement and production launch wiring, assurance signing,
and the query and UI are follow-up work. Principal binding, Host environment
measurement, application aggregation, and recovery from rollback of the entire
Host store are outside the minimal profile.
