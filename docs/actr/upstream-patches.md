# Native RTC, TURN and SQLite dependencies

The native WebRTC stack uses an official upstream Git commit until its required
fixes are available in compatible registry releases. The TURN protocol crates
use published releases. The declarations are defined in the
[root `Cargo.toml`](../../Cargo.toml):

| Crates | Pinned source | Changes |
| --- | --- | --- |
| `webrtc` | `8ea6e2a69b0c4c97f38b0e81e5a77e1c066f3757` | Includes webrtc-rs/webrtc [#928](https://github.com/webrtc-rs/webrtc/pull/928), [#944](https://github.com/webrtc-rs/webrtc/pull/944), and [#946](https://github.com/webrtc-rs/webrtc/pull/946). |
| `rtc`, `rtc-turn`, `rtc-rtp` | The same WebRTC revision; RTC submodule `cd3f08a243ced3714c965070b4fa7105dfe89c0e` | Includes webrtc-rs/rtc [#274](https://github.com/webrtc-rs/rtc/pull/274) and subsequent RTC fixes. |
| `turn`, `stun`, `webrtc-util` | crates.io `0.17.3` | Includes webrtc-rs/webrtc [#896](https://github.com/webrtc-rs/webrtc/pull/896), [#927](https://github.com/webrtc-rs/webrtc/pull/927), [#930](https://github.com/webrtc-rs/webrtc/pull/930), [#932](https://github.com/webrtc-rs/webrtc/pull/932), [#934](https://github.com/webrtc-rs/webrtc/pull/934), and [#936](https://github.com/webrtc-rs/webrtc/pull/936). |
| `sqlx` | `10462fa4414c6c7f1ed1d39bb31678e3b726878c` | Includes [SQLx #4502](https://github.com/transact-rs/sqlx/pull/4502), allowing the patched `libsqlite3-sys` 0.39 line used by `rusqlite` 0.41. |

`webrtc`, `rtc`, and `rtc-rtp` use one Git source so that the SDK and WebRTC
share the same RTC types. Cargo discovers `rtc-turn` and the other RTC protocol
crates inside that checkout's RTC submodule. The TURN server, STUN protocol,
and `webrtc-util` use the same exact published version. Git/path consumers
inherit these dependencies without adding a workspace `[patch]` table.

## Registry consumers

Cargo converts Git dependencies with a `version` to registry dependencies when
packaging. Until compatible upstream releases contain the required fixes,
consumers of published Actr crates must put the RTC and SQLx overrides in their
own workspace root. The CLI's
[generated Rust app manifest](../../crates/actr-cli/fixtures/rust/Cargo.toml.hbs)
contains the required `[patch.crates-io]` entries; existing projects can copy
those entries and merge them with any existing table. These overrides are not
inherited from a published library. See Cargo's
[multiple-location rules](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#multiple-locations)
and [override documentation](https://doc.rust-lang.org/cargo/reference/overriding-dependencies.html).

From a repository checkout, the entries can also be merged into an existing
workspace manifest:

```sh
python3 scripts/actr/apply-upstream-patches.py /path/to/workspace/Cargo.toml
```

The published `sqlx` 0.9.0 requires `libsqlite3-sys <0.38`, which cannot
resolve alongside `rusqlite` 0.41's `libsqlite3-sys 0.39` dependency. The
SQLx override must be present in the consumer's workspace until an upstream
release supports the patched SQLite version. Publishing without client-side
consumer overrides also requires upstream registry releases containing the RTC
fixes; pinning Git revisions alone does not provide that. `turn` 0.17.3 provides
the APIs required by `loran-stun`. The release
train checks this package before uploading Rust crates; its `--dry-run` mode
does not verify registry-resolved Rust packages. The remaining RTC registry
migration is tracked by
[issue #359](https://github.com/Actrium/actrium/issues/359).

The `turn` implementation provides the UDP TURN/STUN profile used here;
TCP/TLS/DTLS TURN, OAuth, USERHASH, DONT-FRAGMENT socket control, and RFC 5780
NAT behavior discovery remain outside this profile. Each deployment exposes one
configured relay address family. This implementation has not received independent
RFC conformance certification.
