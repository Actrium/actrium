# Repository layout

Status: current structural baseline.

The monorepo is organized around visible components first. `crates/` and
`packages/` then provide one consistent home for reusable implementation units,
rather than making each component invent its own package topology.

## Current tree

    actr/
    └── README.md          Public Actr boundary and component-level guidance

    keel/
    └── README.md          Public Keel boundary and component-level guidance

    loran/
    ├── config/            Service composition and local/development configuration
    ├── deploy/            Container and production deployment entry points
    └── docs/              Loran operations and administration documentation

    android/               Android project root
    ios/                   iOS project root
    macos/                 macOS project root
    windows/               Windows project root

    crates/
    ├── actr-*/            Actr runtime, protocol, CLI, and Rust bindings
    ├── actrium-*/         Browser core, sync, vault, and native services
    ├── keel-*/            Keel Rust host/guest support where applicable
    └── loran-*/           Loran daemon, signaling, STUN/TURN, signing, and control

    packages/
    ├── actr-*/            Language bindings and package-facing runtime APIs
    ├── actrium-*/         Browser model, app SDK, system apps, and platform glue
    └── keel-*/            Runtime, host, guest, SDK, devtools, and backends

    tools/
    ├── actr/              Actr-only generation and developer tooling
    └── keel/              Keel-only generation and developer tooling

    contracts/
    ├── actr/              Actor and engine wire contracts
    ├── actrium/           Browser model and service contracts
    ├── ipc/               Cross-process application and platform contracts
    ├── loran/             Service control, discovery, and signaling contracts
    └── sandbox/           Keel host/guest capability contracts

The component roots own public composition, integration configuration, and
component documentation. A buildable library, service, executable, or SDK
belongs in `crates/` or `packages/` according to its toolchain. Root-level
platform projects follow their native ecosystem's conventional layouts.

## Mapping from the current repositories

- **actr/core, actr/crates, actr/src → crates/actr-***: retain existing crate
  boundaries during import; rationalize only afterward.
- **actr/bindings → packages/actr-*** (and `crates/actr-*` for Rust-facing
  bindings). **actr/cli → crates/actr-cli**; generators and developer-only
  tools move under **tools/actr/**.
- **actr/actrix → loran/** plus **crates/loran-***: `loran/` owns the deployable
  service suite; its daemon and reusable signaling, STUN/TURN, signer, control,
  discovery, and SDK units remain normal crates.
- **actro/apps/{android,ios,macos,windows} → android/, ios/, macos/, windows/**:
  each keeps its ecosystem's expected project root rather than living under a
  generic `browser/` directory.
- **actro/crates → crates/actrium-*** and **actro/packages →
  packages/actrium-***. **actro/platform → packages/actrium-platform**.
- **actro/schemas → contracts/actrium/** and **contracts/ipc/**.
- **keel/src → packages/keel-***, preserving the runtime, trusted host,
  untrusted guest, SDK, devtools, and backend boundaries. Rust guest support
  belongs in **crates/keel-***.
- **keel/native → packages/keel-platform/**. There is intentionally no generic
  `native/` root; native code stays with the package or platform that owns it.
- **keel/contracts → contracts/sandbox/**.

## Boundary rules

1. **Actr** remains usable without Actrium applications, Keel, or Loran.
2. **Keel** may depend on Actr contracts, but not on Actrium product state.
3. **Loran** may use Actr protocol packages, but must not depend on a client
   platform application.
4. **Android, iOS, macOS, and Windows** own native lifecycle, packaging,
   permissions, and presentation; shared behavior belongs in a package or
   crate instead.
5. **crates/** and **packages/** contain reusable buildable units only; they do
   not contain deployment composition or platform application roots.
6. **contracts/** contains schemas and generated-boundary definitions, not
   business logic. Cross-component tests live at the repository root;
   component-local tests stay beside the unit they exercise.
