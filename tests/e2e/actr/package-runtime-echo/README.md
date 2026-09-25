# Package Runtime Echo E2E

This scenario validates the local package runtime flow end-to-end:

1. Start a local `actrix` node from the scenario config.
2. Use the Admin API to create a dedicated realm and capture its owner invite;
   the server host joins as the owner (`actr realm join`), invites the client
   host as an editor (`actr realm invite --role editor`), and the client host
   joins with its own Realm client key. Each host signs its Issuer requests
   with its key (`deployment.realm_client_key`).
3. Seed the publisher row used by the server package into the run's SQLite database (production publishers register with a signed `register_publisher` request).
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
- `python3`

`actrix` does not need to be preinstalled. The scenario resolves it in this order:

1. `ACTRIX_BIN`
2. `actrix` from `PATH`
3. Install the in-tree `crates/loran-server` binary when `ACTRIX_BIN` is not already set

The auto-install target is the default cargo user bin directory:

- `$CARGO_HOME/bin`
- `~/.cargo/bin` when `CARGO_HOME` is unset

## Run

```bash
bash tests/e2e/actr/package-runtime-echo/run.sh
bash tests/e2e/actr/package-runtime-echo/run.sh "Hello"
```

Useful environment variables:

- `ACTRIX_BIN` overrides the `actrix` executable path.
- `KEEP_TMP=1` keeps the `.tmp/run-*` directory after the run.
- `CLIENT_TIMEOUT_SECONDS` changes the client wait timeout.
- `RUST_LOG` forwards the log level to the host processes.
