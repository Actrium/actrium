# Package Runtime Echo E2E

This scenario validates the local package runtime flow end-to-end:

1. Start the in-tree `actrix` test fixture from the scenario config.
2. Use the Admin API to create a dedicated realm and capture its owner invite;
   the server host joins as the owner (`actr realm join`), invites the client
   host as an editor (`actr realm invite --role editor`), and the client host
   joins with its own Realm client key. Each host signs its Issuer requests
   with its key (`deployment.realm_client_key`).
3. Seed the service publisher's public package key into the disposable SQLite database. The test-only server supplies a signed Publisher Profile for `e2e.actrium.test` through the Issuer's document-fetch interface, then runs normal signature, pin and Registry acceptance checks. Production binaries have no fixture override and always fetch public HTTPS documents. The client package remains a development identity, verified by its configured public key.
4. Generate a temporary Rust echo service with `actr init/install/gen`.
5. Build and publish the signed server package.
6. Build a local client guest package and run the client host.
7. Assert the client receives the echoed reply.
8. Remove the client's member (`actr realm remove-member`) and assert the same
   client host is refused a credential (`client_revoked`).

## Requirements

- `cargo`
- `curl`
- `jq`
- `sqlite3`
- `python3` 3.11 or later
- `perl`, `rustc`, `lsof`, `timeout`

`actrix` does not need to be preinstalled. The scenario builds its test binary with `cargo test --no-run --bin actrix` and starts only the ignored `package_runtime_fixture` entry point. This avoids depending on public DNS for the virtual publisher domain while exercising the complete server and package runtime. An explicit `ACTRIX_BIN` must name that test binary.

## Run

```bash
bash tests/e2e/actr/package-runtime-echo/run.sh
bash tests/e2e/actr/package-runtime-echo/run.sh "Hello"
```

## Signed Realm creation matrix (PR #305)

```bash
# Both formal creation paths, each against a fresh single-node Loran.
bash tests/e2e/actr/package-runtime-echo/realm-matrix.sh

# Or run one path while iterating.
REALM_MODE=provider bash tests/e2e/actr/package-runtime-echo/run.sh
REALM_MODE=external bash tests/e2e/actr/package-runtime-echo/run.sh
```

The matrix uses `MODE=published` and enables the Provider's authority. It builds
the real package before creating the Realm, so the Profile pins its actual
SHA-256 and Actor type. `REALM_MODE=provider` creates `echo-<run>@localhost`
through the Admin API and verifies the Provider-signed Profile.
`REALM_MODE=external` publishes a separate domain's self-signed Realm keys, uses
the real CLI `create-request` / `sign-profile` commands, and checks the
pending-to-active transition through `/actr/realm-management`.

Both paths resolve and verify the Profile before redeeming owner/editor invites,
use independent Realm client keys, run the real Issuer, signaling and Host Echo
round-trip, repeat the round-trip after restarting the Service Host, remove the
editor and require `client_revoked`, then retire the Realm and require the
client resolver to refuse its retired Profile. Negative cases cover tampered
Profiles, single-use invites, and (external domain) invalid management signatures
and request replay. The test orchestrator translates the verified Profile into
the existing Host runtime configuration; automatic Host resolution is not added.

**Transport boundary:** virtual-domain public documents are read from a
disposable URL-to-document map instead of public HTTPS. The map is used only by
the server test binary and the explicitly invoked CLI lib-test entry point
`realm_runtime_join_fixture`. That entry point runs the production Profile
resolver and join implementation without `--issuer`; membership requests still
go to the running server over HTTP. Document signatures, domain-key acceptance,
management envelopes, membership authorization and client proofs are not
stubbed. Publisher transport keeps the existing signed-document fixture. The
package is built locally and hash-checked, rather than downloaded from its
declared HTTPS URL. Production executables have no document-map override.

The matrix enables real STUN/TURN and uses relay-only RTC, so the Echo also
requires usable Realm-scoped TURN credentials. Its test server permits private
relay peers only when both the existing package-runtime fixture marker and the
Realm document-map marker are set; production keeps its private-peer filter.
The advertised ICE address defaults to the IPv4 address selected by the OS's
default route (determined without sending a packet), with a loopback fallback
when no route exists. Override `REALM_ICE_IP` to select another reachable IPv4
address of the test machine. This changes the advertised test deployment
address, not identity checks or the RTC implementation.

This is protocol/runtime E2E, not public DNS/WebPKI, package-download or UI E2E.
It does not claim coverage for `.actr.alt` local Realms, Provider subdomains,
multi-node infrastructure, periodic external Profile refresh, or persistent
client trust-pin/rollback handling. Those need separate implementation or
deployment tests. Existing multi-Realm isolation and key-lifecycle suites remain
complementary coverage; the matrix does not replace them.

`KEEP_TMP=1` retains local logs and fixtures for diagnosis. The retained directory
contains generated private keys and invites: do not upload the whole directory.

Useful environment variables:

- `ACTRIX_BIN` supplies an already-built `actrix` test binary containing `package_runtime_fixture`.
- `HTTP_PORT` and `ICE_PORT` override the default ports 8081 and 3478.
- `ACTR_TARGET_DIR`, `ACTRIX_TARGET_DIR`, `WORKSPACE_TARGET_DIR` and `TEMP_SERVICE_TARGET_DIR` allow build-cache reuse.
- `KEEP_TMP=1` keeps the `.tmp/run-*` directory after the run.
- `CLIENT_TIMEOUT_SECONDS` changes the client wait timeout.
- `RUST_LOG` forwards the log level to the host processes.
