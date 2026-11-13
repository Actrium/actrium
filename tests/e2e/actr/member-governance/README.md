# Member governance client acceptance

The Rust SDK, Swift UniFFI bindings and Kotlin UniFFI bindings call the real
loopback HTTP Issuer, membership database and embedded Signer. Each language run
gets a fresh fixture and a one-time operator-issued owner recovery invitation in
a mode-0600 temporary file. The harness deletes the invitation on exit.
Set `ACTR_GOVERNANCE_FIXTURE_BIN` to the absolute path of an integration-test
binary already built from this checkout to reuse it without rebuilding.

Rust integration coverage runs without an external service:

```sh
cargo test --locked -p loran-issuer --test integration_test device_membership::governance
```

Generate matching native libraries/bindings before language acceptance. Swift
bindings and XCFrameworks are ignored build outputs in this monorepo:

```sh
ACTR_BUILD_PROFILE=debug ACTR_XCFRAMEWORK_TARGETS=macos bash packages/actr-swift/build-xcframework.sh
bash tests/e2e/actr/member-governance/run.sh swift

ACTR_BUILD_ANDROID_NATIVE=false bash packages/actr-kotlin/build-android.sh
ACTR_GOVERNANCE_LIBRARY="$PWD/target/aarch64-apple-darwin/debug/libactr.dylib" \
  bash tests/e2e/actr/member-governance/run.sh kotlin
```

The Kotlin test runs on the host JVM with desktop JNA and the matching host
library; it does not exercise Android Keystore. Select the host library path for
your platform and use JDK 17 plus an Android SDK for the Gradle build. Run this
test separately from tests that deliberately simulate a missing native library.
The language tests are skipped in ordinary SDK test runs without the fixture.

Coverage includes editor/viewer invitations, all five governance calls, owner
permission boundaries, stable error codes, repeat calls, exactly one owner after
transfer, removal of all member devices and outstanding invitations, and leaving.
Rust additionally loses successful HTTP responses after commit, races owner
transfers, exercises authorized owner recovery, checks Actor invalidation and
unrelated-member isolation, and verifies credential role propagation.

Retries retain the protocol's existing semantics: inviting again creates a new
invitation; setting the same role and owner-authorized member removal can be
repeated. A completed transfer can make the previous owner unauthorized, and a
completed leave revokes its signer. An unknown transport result is not a refusal;
reconcile membership before retrying. Role changes apply to the next issued Actor
credential, and successful Provider revocation does not assert immediate closure
of every peer session.
