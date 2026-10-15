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

Useful environment variables:

- `ACTRIX_BIN` supplies an already-built `actrix` test binary containing `package_runtime_fixture`.
- `HTTP_PORT` and `ICE_PORT` override the default ports 8081 and 3478.
- `ACTR_TARGET_DIR`, `ACTRIX_TARGET_DIR`, `WORKSPACE_TARGET_DIR` and `TEMP_SERVICE_TARGET_DIR` allow build-cache reuse.
- `KEEP_TMP=1` keeps the `.tmp/run-*` directory after the run.
- `CLIENT_TIMEOUT_SECONDS` changes the client wait timeout.
- `RUST_LOG` forwards the log level to the host processes.
