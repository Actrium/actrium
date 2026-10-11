# Native RTC dependencies and TURN overrides

## Temporary WebRTC fork: merge blocker

The native SDK temporarily uses `kaito-harry/webrtc` at
`634b776e03938bc6339b399ca12f1162033c6dd1`, which advances its RTC submodule to
`cd3f08a243ced3714c965070b4fa7105dfe89c0e`. The upstream change is tracked in
[webrtc-rs/webrtc #944](https://github.com/webrtc-rs/webrtc/pull/944).

**Do not merge Actrium/actrium #325 while these dependencies use the fork.**
Replace it with a suitable official upstream commit or published versions,
refresh the affected lockfiles, and verify the complete dependency graph before
merging. All direct RTC dependencies must continue to use the same source.

The dependency declarations and remaining server overrides are defined in the
[root `Cargo.toml`](../../Cargo.toml):

| Crates | Pinned source | Changes |
| --- | --- | --- |
| `webrtc` | Fork `634b776e03938bc6339b399ca12f1162033c6dd1` | Includes merged webrtc-rs/webrtc [#928](https://github.com/webrtc-rs/webrtc/pull/928), preserving allocation credentials; temporarily includes [#944](https://github.com/webrtc-rs/webrtc/pull/944). |
| `rtc`, `rtc-turn`, `rtc-rtp` | The same WebRTC fork revision; RTC submodule `cd3f08a243ced3714c965070b4fa7105dfe89c0e` | Includes merged webrtc-rs/rtc [#274](https://github.com/webrtc-rs/rtc/pull/274), fixing TURN channel numbers, and subsequent RTC fixes. |
| `turn`, `stun`, `webrtc-util` | `c229349e67cc235e6737bbf92bd7dd48fdc135fa` | Merged webrtc-rs/webrtc [#936](https://github.com/webrtc-rs/webrtc/pull/936), including [#927](https://github.com/webrtc-rs/webrtc/pull/927), [#930](https://github.com/webrtc-rs/webrtc/pull/930), [#932](https://github.com/webrtc-rs/webrtc/pull/932), [#934](https://github.com/webrtc-rs/webrtc/pull/934) and the earlier [#896](https://github.com/webrtc-rs/webrtc/pull/896). |

`webrtc`, `rtc`, and `rtc-rtp` are explicit `git` + `rev` dependencies in
`[workspace.dependencies]`. Cargo discovers RTC and its protocol crates inside
that same WebRTC checkout, preserving one source and one set of public types.
Git/path consumers of the SDK inherit these dependency declarations and do not
need client-side `[patch]` tables.

## Legacy TURN server overrides

The TURN server, STUN protocol, and `webrtc-util` use one upstream Git source
and version because their public types cross the service boundary. These
server overrides remain in `[patch.crates-io]`. Cargo reads `[patch]` only from
the final workspace root, so external server consumers (including SDK
`test-utils` builds that enable server fixtures) must still apply this table
in their own root `Cargo.toml`. The repository's independent workspaces
repeat it for their server fixtures. See the [Cargo override documentation](https://doc.rust-lang.org/cargo/reference/overriding-dependencies.html).

## Registry publication

Explicit Git dependencies solve source builds, including independent SDK
consumers. They do not make the fixes available to crates.io consumers:
Cargo replaces `git` + `version` dependencies with registry dependencies when
packaging. Registry publication therefore also requires published upstream
versions containing the required fixes. See [Cargo's multiple-location rules](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#multiple-locations).

The Git `turn` implementation provides the UDP TURN/STUN profile used here;
TCP/TLS/DTLS TURN, OAuth, USERHASH, DONT-FRAGMENT socket control, and RFC 5780
NAT behavior discovery remain outside this profile. Each deployment exposes one
configured relay address family. This implementation has not received independent
RFC conformance certification.
