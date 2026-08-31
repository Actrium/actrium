# Repository layout

Status: proposed structural baseline.

The monorepo is organized by architectural ownership first, not by programming
language. Rust, TypeScript, Kotlin, Swift, and native platform code stay near
the subsystem they implement.

## Proposed tree

    engine/
    ├── core/             Actr protocol, runtime, framework, and platform traits
    ├── gateway/          Actrix services, deployment, and administration
    ├── bindings/         C ABI, FFI, Kotlin, Python, Swift, TypeScript, and Web
    ├── cli/              Actr command-line interface
    └── tooling/          Engine-specific generators and build tools

    browser/
    ├── apps/             Android, iOS, macOS, and Windows entry points
    ├── core/             Browser orchestration and native browser services
    ├── packages/         Reusable browser packages during the first migration
    ├── platform/         Platform adapters shared by the applications
    ├── system-apps/      Built-in Actrium applications
    ├── sync/             Browser synchronization service and client
    └── vault/            Browser storage and secrets boundary

    ui/keel/
    ├── runtime/          Keel orchestration and shared runtime model
    ├── host/             Trusted host API and render integration
    ├── guest/            Sandboxed application API
    ├── native/           React Native and native platform integration
    ├── backends/         Hermes, JSC, QuickJS, VM, and WASM backends
    ├── sdk/              Application-facing Keel SDK
    ├── devtools/         Inspection and development experience
    └── tooling/          Keel CLI, code generation, and build support

    contracts/
    ├── actr/             Actor and engine wire contracts
    ├── browser/          Browser model and service contracts
    ├── ipc/              Cross-process application and platform contracts
    └── sandbox/          Keel host/guest capability contracts

## Mapping from the current repositories

- **actr/core, actr/crates, actr/src → engine/core/**: keep crate boundaries
  during import; rationalize only afterward.
- **actr/actrix → engine/gateway/**: Actrix remains a named engine service, not
  a top-level product.
- **actr/bindings → engine/bindings/**: preserve language-specific release
  boundaries.
- **actr/cli → engine/cli/** and **actr/tools → engine/tooling/**.
- **actro/apps → browser/apps/**: product entry points become Actrium apps.
- **actro/crates → browser/core, browser/sync, browser/vault/**: split by
  existing responsibility, not language.
- **actro/packages → browser/packages/** and later **ui/keel/**: keep a staging
  area first; move packages only when ownership is clear.
- **actro/platform → browser/platform/**.
- **actro/schemas → contracts/browser/** and **contracts/ipc/**.
- **keel/src → ui/keel/runtime, host, guest, sdk, devtools/**: preserve the
  host/guest trust boundary.
- **keel/native → ui/keel/native/** and **ui/keel/backends/**: separate platform
  glue from execution backends.
- **keel/contracts → contracts/sandbox/**.

## Boundary rules

1. **engine/** must remain usable without the browser or Keel.
2. **ui/keel/** may depend on engine contracts, but not browser product state.
3. **browser/** composes the engine and Keel and owns platform applications.
4. **contracts/** contains schemas and generated-boundary definitions, not
   business logic.
5. Cross-layer tests live at the repository root; subsystem-local tests stay
   beside their implementation.
