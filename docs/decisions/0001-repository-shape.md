# 0001: Organize the monorepo around component roots and package entrances

Status: proposed

## Context

Actr, Actro, Keel, and the service currently called Actrix live in separate
repositories, although the browser increasingly composes the first three as
one product. A merged repository needs to show those relationships without
erasing the boundaries or conflicting with native-platform conventions.

## Decision

Use visible root directories for **actr/**, **keel/**, **loran/**, and the
native application roots **android/**, **ios/**, **macos/**, and **windows/**.
Use **crates/** as the single home for Rust implementation units and
**packages/** as the single home for reusable units in other ecosystems.

`loran/` is the default and reference shore-side service composition. Its
operational configuration and deployment belong there; the daemon and its
reusable services remain normal crates under `crates/`.

There is no generic `browser/` wrapper: Actrium is the product represented by
the platform applications and their shared packages. There is likewise no
generic `native/` root.

## Consequences

- The roots that people navigate first are parallel components, not a mixture
  of product layers and implementation languages.
- Platform projects can retain their conventional Android, Xcode, and Windows
  layouts.
- Actr can still be built and released independently as the reusable
  foundation.
- Keel's host/guest security boundary remains explicit through package and
  contract ownership.
- Loran can ship as the normal service offering while also serving as a clear
  reference deployment.
- Root build orchestration will coordinate multiple language toolchains and
  package managers.
