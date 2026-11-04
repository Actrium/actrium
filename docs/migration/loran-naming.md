# Loran naming migration

The service suite now uses Loran consistently across Rust packages, executables,
configuration, storage, observability, deployment and test tooling. This is a
breaking naming change; upgrade the composed services and their configuration
together.

## Packages and commands

- Build and run the server with `cargo build -p loran-server` and `loran --config ...`.
- Reusable service packages use `loran-*`, including `loran-admin` and
  `loran-issuer`. The control implementation remains in `crates/loran-control`.
- The deployment helper package is `loran-deploy`; its executable remains
  `deploy`.
- Integration fixtures use `actr-mock-loran` and the `mock-loran` executable.
- The public Rust configuration type is `LoranConfig`.

External consumers must update their Cargo dependencies and Rust imports. The
server package is `loran-server` because the unqualified
`loran` registry name belongs to another project. Its Rust library is
`loran_server`, and its executable remains `loran`. The release train publishes
the renamed packages in dependency order before the
server, and publishes `loran-linux-*`, `loran-macos-*` and `loran-deploy-linux-*`
assets from `Actrium/actrium`.

## Configuration and deployment

1. Stop the service and back up its configuration, databases and Signer key
   material before upgrading.
2. Start from `loran/config/loran.example.toml`. Rename the internal shared-secret
   field to `loran_shared_key`; preserve its value and the separate TURN,
   renewal, control and Signer secrets.
3. Use the `LORAN_*` environment variables, including `LORAN_SIGNER_KEK`,
   `LORAN_REPOSITORY`, `LORAN_HEALTH_URL` and `LORAN_HEALTH_WAIT_SECONDS`. Preserve
   the existing Signer KEK value so stored private keys remain decryptable.
4. Update service units, users, groups, executable paths and PID/log paths to the
   Loran installation layout: `/opt/loran`, `/etc/loran`, `bin/loran` and
   `logs/loran.pid`. Preserve the service account's access to configuration, data
   and keys. Keep custom paths explicit in the service configuration.
5. With the service stopped, move the existing main SQLite database to
   `{sqlite_path}/loran.db`. Preserve a consistent database including any WAL
   state; do not start against an empty database in place of the existing one.
   The schema and stored Realm identities are unchanged by this naming change.
   The Signer's effective SQLite filename remains `signer_keys.db` in the
   instance data directory; its PostgreSQL
   database default is now `loran_signer`, so set the PostgreSQL `database` and
   `username` fields explicitly when retaining an existing database or role
   with a different name.
6. Update dashboards, scraping rules and alerts to the `loran_` metric prefix,
   `loran::` recording targets and Loran service names. Update browser Admin UI
   integrations to the renamed local-storage keys.
7. Restart and verify the existing Realms, identities, Signer access and health
   endpoints before admitting clients.

Existing configurations must use the new names; there are no old-name aliases
or automatic database-file moves. HTTP and gRPC endpoint paths, protobuf field
numbers and the relay protocol are unchanged.

## Example endpoints

Checked-in public service addresses use `loran.example.com`. This is a reserved
example domain, not a deployed service. Replace it with the operator's actual
signaling, Issuer and ICE endpoints before running a client. Local integration
tests continue to start their own loopback services.
