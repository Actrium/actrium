# Trusted Program package loading

This is the first F01 #364 / #366 delivery replacing PR #182. A native
installer authenticates a complete Program before handing local bytes to
the controlled runtime. Package resolution through the retired discovery
relation is removed from the implementation plan.

## Authentication boundary

`actrium_core::package_install` reuses the protocol Publisher acceptance
rules and `actr_pack::program` validation. The latter is the verification
portion of the existing #348 implementation; its complete build/sign tooling
and runtime integration continue under #348.

The native verifier is behind actr-pack's explicit `program` feature, enabled
by actrium-core. Existing WASM consumers keep the base package dependency
without pulling native proto tooling into the browser target; they reject
unsupported complete Program declarations rather than ignoring them.

1. The Host selects an HTTPS package source. A Realm caller supplies the
   exact `href`, SHA-256 and Actor type from F02's already accepted Profile.
   `PackageSourceInfo` is not a Realm resolver or evidence of Realm trust.
2. A bounded download is inspected for its expected digest, archive structure,
   payload integrity and canonical Publisher identity. Inspection returns no
   executable contents.
3. Fetch `https://<publisher>/.well-known/actr` using Web PKI. Each redirect
   must retain HTTPS and pass the same URL policy, with at most five hops.
   The original Publisher remains the identity checked by Rust.
4. Verify the signed Publisher member, validity, sequence and key history.
   Save accepted pins independently of package success, so a rejected package
   cannot undo an observed revocation. Corrupt saved pins fail closed. An
   unchained identity change is reported for an explicit Host decision.
5. Find the manifest key in the current signed Profile. Listed active and
   retired keys can verify packages; absent keys are revoked. Verify the
   signature, derived key ID, all files, Program declarations, Component ABI
   and imports, closed JavaScript bundle, contracts and v1 Host compatibility.
6. Return one `VerifiedProgramInfo`: identity/digest, typed declaration JSON,
   Guest source, Service bytes and signed resources, all from that archive.
   Capability declarations are requests; session authorization supplies grants.

There is no Registry publication lookup and no Publisher private key on the
consumer. Realm, Publisher and download domains can differ.

## Apple production entry

`AppleTrustedPackageLoader` is the shared Apple downloader and trust-store
adapter. It checks responses while receiving them, caps package bytes at
256 MiB and descriptors at 64 KiB, propagates cancellation and saves only
verified archives atomically under their digest. Publisher acceptance is
serialized with a file lock and atomically persisted across launches.
Publisher documents have a 60-second memory TTL; signature/expiry/pins are
rechecked on every use, and concurrent or failed refresh attempts are bounded
to one per Publisher per minute.

In macOS, entering an explicit `https://.../program.actr` address uses this
loader. The Tab receives the verified Guest through the existing local-source
runtime property and retains the authenticated Service/resources for runtime
consumers. Navigation IDs and cancellation protect both success and error
delivery after a later navigation or Tab closure. The install operation does
not grant business capabilities automatically.
`ActriumTrustedProgramCoreBridge` adapts the generated Rust records to native
DTOs, keeping the Tab and view model outside the FFI boundary.

`openAcceptedRealmPackage` is the adapter for F02 #372. That resolver still
owns first Realm trust, persisted baselines, refresh and retirement. A raw
Realm URL is not converted into an unsigned package lookup. The full
Front/Service session remains under #324. Other platform loaders consume the
shared verification API when their production adapters are connected.

The staging performed here is not the persistent release ledger (#367),
offline/reference-aware cache (#368), development-source policy (#370), or
Program-level revocation policy (#371). Existing local development imports
remain part of that separate source-policy work.

## Reproducible checks

The default Rust integration fixture builds a real Component ABI from the
repository WIT using `wit-component`, and signs the complete archive with
ephemeral test keys. It exports only the signed package and public Publisher
descriptor when requested, never private keys:

```sh
cargo fmt
cargo clippy --fix --lib --bins --all-targets --allow-dirty
cargo test -p actr-pack -p actrium-core --lib
ACTRIUM_EXPORT_TEST_FIXTURE=/tmp/actrium-package-fixture \
  cargo test -p actrium-core --test package_install
scripts/actrium/build-actrium-rust-bindings.sh apple-macos
ACTRIUM_PACKAGE_TEST_FIXTURE=/tmp/actrium-package-fixture \
  swift test --package-path packages/actrium-platform/apple/ActriumCore
npm run verify:macos
xcodebuild -workspace macos/Actrium.xcworkspace -scheme Actrium \
  -destination 'platform=macOS' CODE_SIGNING_ALLOWED=NO test \
  -only-testing:ActriumTests/TrustedProgramNavigationTests
```

Use the rustup toolchain for multi-target builds: its installed macOS targets
are independent of a Homebrew `rustc` installation. Fresh macOS checkouts also
need the JavaScript dependencies, `pod install` in `macos`, and generated
ActriumCore/ActriumVault XCFrameworks before the App test target builds.

To additionally validate an SDK source-built Service from #348's Program
example, build that example and run:

```sh
ACTRIUM_TEST_SERVICE_COMPONENT=/absolute/path/to/bin/service.wasm \
  cargo test -p actrium-core --test package_install \
  real_source_built_service_is_accepted_when_requested -- --ignored
```

The Rust suite covers bad selections, signatures/key IDs, payload tampering,
dangerous ZIP paths, Host/ABI policy, key removal, root replacement, rollback
and corrupt saved pins. Apple integration tests exercise the production
downloader with the generated signed fixture, restart, failed installation,
streaming limits, cancellation and redirect policy. App tests cover the native
entry and navigation races; they do not assert the separate Attach/lease flow.
