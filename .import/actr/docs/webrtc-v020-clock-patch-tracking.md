# webrtc v0.20 Runtime Clock-Patch Tracking

Status: Stock crates.io v0.20.0 selected; Clock levels A/C unavailable, Actr policy clock retained
Opened: 2026-08-04; local backport removed: 2026-08-10
Related: `docs/webrtc-v020-test-contract.md` §4, `docs/webrtc-v020-capability-matrix.md`,
canonical `rfcs/0504-webrtc-v020-rtc-controller-architecture.md` §"Clock capability
boundary" (lines 1380-1403)

This note records the released-v0.20 gap, the former local backport, the decision
to return to the unmodified crates.io release, and the remaining limitations.

---

## 1. The dependency

Stage 5 needs two clock capabilities that the current webrtc-rs v0.20.0 public
API does not provide:

- **`ManualPolicyClock` / deterministic driver deadlines** — Controller and
  recovery tests need to advance time explicitly and have the PeerConnection
  driver observe that advancement.
- **Sans-I/O fast-forward / replay** — `SansIoProtocolRig` (the optional
  deterministic T2 tool) needs ICE/DTLS/SCTP protocol steps to run on a virtual
  clock, not wall-clock time.

Both depend on a **controllable `Runtime::now()` / clock seam** in webrtc-rs
v0.20: every clock read on the production path must go through the `Runtime`
abstraction so a test can substitute a virtual clock.

## 2. Published source and upstream fix

The **published crates.io v0.20.0 source** has no `Runtime::now()`. Its trait abstracts *waiting*
(`spawn`, `sleep`, `interval`, `block_on`, `yield_now`, socket/DNS helpers), not
the current time. There is no `Runtime::now() -> Instant` (or equivalent) to
override. (`runtime/mod.rs`)

**`runtime-mock` has a `VirtualClock`, but only for timers.** `MockRuntime`
drives `sleep`/`interval`/`timeout` from a manually advanced `VirtualClock`
(`rt.clock()`, `clock.advance(delta)`, `clock.now() -> Duration`). That controls
*its own* timers — it does **not** control the production driver's clock reads.

**The production driver reads `Instant::now()` directly, bypassing `Runtime`.**
Seven read sites across six production-path locations (matching the canonical "six clock reads"):

| Site | File:line |
| --- | --- |
| Driver timeout arithmetic | `peer_connection/driver.rs:322` |
| Driver `handle_timeout(Instant::now())` | `peer_connection/driver.rs:363` |
| Driver UDP ingress `now` | `peer_connection/driver.rs:532` |
| Default timeout duration | `peer_connection/driver.rs:1321` |
| TCP transport ingress | `transports/tcp_transport.rs:197` |
| TURN relay ingress (first read) | `transports/turn_relayer.rs:661` |
| TURN relay ingress (second read) | `transports/turn_relayer.rs:798` |

Upstream subsequently landed commit `67091285dd9ded357da02c05c99bab39cb61e74f`
(`C6-01 — Add Runtime::now() and route the driver through it`, 2026-08-07),
after the v0.20.0 release. Actr now consumes the unmodified crates.io v0.20.0
package, so that later upstream seam is intentionally not present locally.

## 3. Blocker status

| Clock acceptance level (canonical) | Status | Evidence |
| --- | --- | --- |
| **A — create the seam** (default `Runtime::now()`, driver/TCP/TURN routing) | **Unavailable on stock v0.20.0** | no local source patch; upstream fix landed after v0.20.0 |
| **B — deterministic Actr policy time** | **Partial** | `ManualPolicyClock` and T1 exact-deadline tests remain; high-level driver clock injection is unavailable |
| **C — validate protocol time** (Sans-I/O fast-forward/replay across ICE/DTLS/SCTP) | **BLOCKED** | protocol-core paths call `Instant::now()` internally; no injection point |

Only Actr-owned policy deadlines are deterministic. `runtime-mock` MUST NOT be
described as high-level or bit-exact protocol replay under the stock dependency.

## 4. Resolution and maintenance action

- Keep `webrtc`, `rtc`, and `rtc-rtp` pinned to the exact crates.io `0.20.0`
  releases, without a local path override.
- Do not expose or test `Runtime::now()` through Actr runtime wrappers because
  it is not part of the published v0.20.0 trait.
- Re-evaluate Clock level A when Actr upgrades to a release containing upstream
  `67091285dd9d` or an equivalent stable implementation.

## 5. What Stage 5 must NOT assume

- Do not infer Clock level C from Actr's policy clock. The stock high-level
  driver and protocol-core code may still read wall time directly.
- Do not mark packet-level ICE/DTLS/SCTP replay as bit-exact until a dedicated
  Clock-level-C source audit and deterministic test proves it.

---

## 6. Historical Stage 5 decision (2026-08-05): defer the vendor/fork

**Decision: do not vendor/fork `webrtc-0.20` for `Runtime::now()` in Stage 5.**
Revisit at the Stage 7 release gate or when the Sans-I/O deterministic T2 rig
is scheduled.

Reasoning:

1. **Item 10's seams do not need the clock patch.** `RuntimeFaultProbe`
   (socket/task/timer boundary metrics + connect-fail / UDP-blackhole /
   driver-exit injection) and `DeterministicNetwork` (signaling-level
   loss/delay/duplicate/partition rules) are implemented entirely on the
   existing `Runtime` trait surface and the `SignalingTransport` seam
   (`v020/fault.rs`). Neither reads the driver's clock.
2. **The clock patch only gates the optional Sans-I/O fast-forward rig**
   (clock levels B/C). No Stage 5 exit criterion depends on bit-exact
   protocol replay; recovery semantics are covered by reducer-level tests
   (pure input→effect transitions, no wall clock) plus real-PC interop rigs
   with explicit deadlines.
3. **Fork cost is disproportionate to seam-only scope.** The seven read sites
   span `peer_connection/driver.rs` and two transport modules; a
   `[patch]`-section fork would have to be carried and rebased through every
   upstream upgrade, for a capability Stage 5 explicitly defers ("do not fix
   a loss/reorder or device matrix at this stage").
4. **Upstream-first remains the plan of record** (§4): open the
   `Runtime::now()` issue/contribution; adopt when it lands.

Consequences carried forward:

- Controller/reducer time tests keep using explicit semantic deadlines; the
  inv24 timer facade remains the only Actr-side timer path.
- `runtime-mock`'s `VirtualClock` MUST NOT be cited as protocol-replay
  evidence (§5).
- The deterministic T2 Sans-I/O rig stays blocked at clock level A until the
  upstream seam exists; that is now an explicit pre-condition in the recovery
  test plan (docs/webrtc-v020-recovery-test-plan.md).

## 7. Former Stage 7 backport and rollback

Stage 7 temporarily superseded the Stage 5 deferral:

- upstream supplied the required seam in `67091285dd9d`;
- Actr backported only the compatible high-level clock changes to the exact
  crates.io v0.20.0 package;
- production wrappers delegate `Runtime::now()` instead of falling back to a
  separate wall clock;
- T1 owns `ManualPolicyClock`, and T2 proves a real high-level driver consumes
  `Runtime::now()`;
- Clock level C remains explicitly out of scope until the protocol core is
  independently audited and validated.

On 2026-08-10 the local path dependency and backport were removed. The current
build uses stock crates.io v0.20.0; only `ManualPolicyClock` remains available
for deterministic Actr policy tests.
