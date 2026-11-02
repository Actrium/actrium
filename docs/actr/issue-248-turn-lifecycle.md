# Issue #248: TURN allocation lifecycle validation

Validated on 2026-11-02 against Actrium main
`ce8b2748ea726114c5cc7d2adaef9e6739470e6a`, plus the test/CI changes
accompanying this record. No production behavior or dependency changes are
needed for the reachable-server cases below. A `test-utils` constructor adds
allocation-close observations through the same production server configuration.

## Baseline and scope

- Client: crates.io `webrtc`, `rtc`, and `rtc-turn` **0.21.1**.
- Server: `turn`, `stun`, and `webrtc-util` **0.17.2**, all pinned to
  `https://github.com/webrtc-rs/webrtc` revision
  `ae66ebd503f013a1f7360a3fd45a8cf83993ee2b` in the unchanged root lockfile.
- Application crates: `actr-hyper` and `actrix-turn` **0.10.7**.
- Environment: Debian 13, Linux x86_64; Rust **1.95.0**
  (`59807616e 2026-04-14`), protoc **29.3**.

The two causes have different fixes:

1. [Actrium #278](https://github.com/Actrium/actrium/pull/278) supplies
   [upstream #896](https://github.com/webrtc-rs/webrtc/pull/896): an identical
   authenticated Allocate transaction replays the original success response;
   a different transaction on the same five-tuple still receives 437.
2. [Upstream #912](https://github.com/webrtc-rs/webrtc/pull/912) ports the
   retirement fix from [#895](https://github.com/webrtc-rs/webrtc/pull/895)
   into the v0.21 branch and lets `close()` give the driver a bounded chance to
   send the releases. The resolved 0.21.1 source queues authenticated
   `Refresh(LIFETIME=0)` and drains it before rebinding or dropping old sockets.

The lifecycle tests use the production Actr RTC manager, recovery controller,
PeerConnection factory, PeerGate RPC path, and embedded TURN server over real
loopback UDP. ICE policy is relay-only. The fixture supplies local WebSocket
signaling and test credentials; this is not an Android/iOS UI or deployed
signaling-service run. No allocation table, relay transport, or release result
is mocked, and the test does not send cleanup Refresh requests on behalf of
the application.

The independent PR review identified that a UDP rebind can legally reuse the
old source port. A table lookup by five-tuple alone would then mistake the new
allocation for the old one. The regressions now consume the server's actual
`alloc_close_notify` events for each round before advancing to the next one,
matching the recorded tuple, relay address, and username. The raw UDP replay
case deterministically allocates again on the same socket before consuming
the old close event, verifies the replacement remains active, and then checks
its own close. Notifications only observe cleanup; they do not cause it.

## Commands and results

Run from the repository root with the stated toolchain and protoc on `PATH`.
The local run used `CARGO_BUILD_JOBS=4 CARGO_PROFILE_DEV_DEBUG=0`.

```sh
# Original probes on the unmodified main test source:
cargo +1.95.0 test --locked -p actr-hyper --features test-utils \
  --test turn_credential_rotation issue_248 \
  -- --include-ignored --nocapture --test-threads=1
# 3 passed, 0 failed, 0 ignored; 2 filtered out (15.59 s).

# Final suite, with lifecycle regressions enabled by default:
cargo +1.95.0 test --locked -p actr-hyper --features test-utils \
  --test turn_credential_rotation -- --nocapture --test-threads=1
# 6 passed, 0 failed, 0 ignored, including deterministic five-tuple reuse.

cargo +1.95.0 test --locked -p actrix-turn --lib --test udp_interop \
  -- --nocapture --test-threads=1
# 30 unit tests and 3 real UDP interop tests passed; none ignored.

cargo +1.95.0 fmt --all -- --check
cargo +1.99.0 clippy --locked -p actr-hyper --features test-utils \
  --test turn_credential_rotation -- -D warnings
python .github/scripts/ci_gate_test.py
python .github/scripts/check_cjk.py
git diff --check
```

All checks above passed. Clippy uses the repository's CI toolchain, Rust
**1.99.0** (`b940084d7 2026-09-28`). An additional Clippy run on 1.95.0 failed
on existing `clippy::nonminimal_bool` diagnostics in unchanged
`crates/loran-signaling/src/server.rs:785` and `:1765`; the same strict command
passes on 1.99.0. This does not affect the successful 1.95.0 test execution.
The CI workflow self-test initially lacked the `sqlite3` executable; it passed
after providing the Debian SQLite CLI locally, without modifying the repository.

| Case | Observed result |
| --- | --- |
| Allocate replay and tuple reuse | Byte-identical success; same relay; exactly 1 allocation; distinct transaction returns 437; explicit Refresh(0) control leaves 0; same-tuple replacement stays active while the old close is observed |
| Peer close | 2 allocations initially; both actual close events received and total 0 after application close |
| One ICE restart | Both old close events received; 2 current allocations; total 0 after final application close |
| Three consecutive ICE restarts | Each round: both old close events received, total 2; final close: both close events received, total 0 |
| Plain restart integration | Business RPC recovers and fresh TURN authentication occurs |
| Credential rotation integration | New credentials reach the real TURN authenticator and business RPC recovers |

Each lifecycle scenario verifies bidirectional business RPC. Restart checks
also require advancing ICE generations on the same PeerConnections, so a
silent hard rebuild cannot substitute for testing ICE rebind. Close event
observations allow at most 3 seconds after recovery/close; the entire scenario
has a 180-second limit, below the server's 600-second default allocation
lifetime. Counts are captured before fixture/server shutdown, preventing
shutdown or natural expiry from satisfying the release assertions.

CI runs `turn_credential_rotation` in the realtime shard. Its nextest retry
override is zero, ahead of the general integration-test retry policy.

## Closure and limits

The original replay and prompt retirement regressions pass on the current
dependencies, and application recovery plus consecutive rebinds are verified.
After the accompanying regression/CI changes are reviewed and merged, these
results support closing [#248](https://github.com/Actrium/actrium/issues/248)
for the tested native application paths. This local validation does not itself
close the issue or establish that remote CI has passed.

- UDP release remains best effort when the old socket/server is unreachable or
  the driver cannot flush before its close deadline. Server TTL is still the
  fallback; this run does not claim release through a failed network.
- The original two Android log lines lack transaction/five-tuple evidence;
  this run cannot retrospectively classify each line. Android/iOS device E2E,
  deployment validation, and the full workspace test suite were not run.
- The fixture logs `signaling: Inbound channel closed` during teardown; the
  lifecycle assertions pass, and the original TURN allocation error does not
  appear. This is not a claim that the whole application emits no errors.
- Published-package migration remains
  [#359](https://github.com/Actrium/actrium/issues/359). Git pins and both
  workspace lockfiles are unchanged. These results apply to the pinned
  checkout, not to crates.io `turn 0.17.2` without the Git fix.
