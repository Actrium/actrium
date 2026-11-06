# Actr import plan

Status: accepted for the initial Actr migration.

## Goals

- Preserve the useful `main` history while excluding obsolete path-only
  history, old branches, and release tags.
- Preserve author and committer names, timestamps, messages, and parent
  relationships for retained commits.
- Replace known contributor identities with their GitHub noreply address,
  leave the single historical `root` identity empty, and redact email
  addresses in commit messages. Source files are not rewritten merely because
  they contain email-shaped test data or package metadata.
- Move the current tree into the monorepo ownership layout without pretending
  that the old repository already used that layout.
- Leave the source repository unchanged.

## History preparation

The import is prepared in an isolated clone of the source repository:

1. Start from `main` only. Do not import source branches or tags.
2. Keep every path present at source `main`, plus predecessor paths found
   through Git rename detection.
3. Remove all other paths from the rewritten history and prune commits that
   consequently become empty or degenerate.
4. Map `author_email` and `committer_email` to verified GitHub noreply
   addresses, leave `root` empty, redact email addresses in commit messages,
   and remove invalidated commit signatures.
5. Prefix the prepared source tree with `.import/actr/` and merge it as an
   unrelated history.

The temporary prefix makes the import boundary explicit and keeps every
retained pre-migration snapshot internally consistent. A following relocation
commit moves the current snapshot into its final directories. Git can then
follow the relocation while old commits continue to describe the old
repository layout truthfully.

## Current-tree mapping

| Source path | Monorepo path | Notes |
| --- | --- | --- |
| `src/`, root package manifest and README | `crates/actr/` | Public Rust facade crate |
| `core/<name>/` | `crates/actr-<name>/` | Preserve current crate boundaries |
| `crates/actr-cabi/` | `crates/actr-cabi/` | Existing reusable crate |
| `cli/` | `crates/actr-cli/` | CLI is a normal buildable crate |
| `bindings/ffi/` | `crates/actr-ffi/` | Rust FFI implementation |
| `bindings/cabi/` | `packages/actr-cabi/` | C ABI consumers and wrappers |
| `bindings/kotlin/` | `packages/actr-kotlin/` | Kotlin package root |
| `bindings/python/` | `packages/actr-python/` | Python package root |
| `bindings/swift/` | `packages/actr-swift/` | Swift package root |
| `bindings/typescript/` | `packages/actr-typescript/` | TypeScript workload package |
| `bindings/web/` | `packages/actr-web/` | Coupled web SDK workspace; split later only if useful |
| Service suite `crates/<name>/` | `crates/loran-<name>/` | Reusable Loran crates and their public package names |
| Service suite `deploy/` | `loran/deploy/` | Deployable service composition |
| Service suite `docs/` | `loran/docs/` | Operations and service documentation |
| Service suite `config.example.toml` | `loran/config/loran.example.toml` | Canonical Loran example configuration |
| Service suite `benchmarks/` | `tests/benchmarks/loran/` | Service-suite performance checks |
| Service suite `scripts/` | `scripts/loran/` | Loran operational workflows |
| `testing/*/` | `crates/<package-name>/` | Buildable Rust test support crates |
| `tools/` | `tools/actr/` | Actr generators, linters, and developer tools |
| `examples/` | `examples/actr/` | Component examples |
| `e2e/` | `tests/e2e/actr/` | Actr end-to-end suites |
| `experiments/` | `examples/actr/experimental/` | Clearly non-reference exploratory code |
| `docs/` | `docs/actr/` | Actr-specific implementation documentation |
| `rfcs/` | `rfcs/actr/` | Existing Actr proposals |
| `scripts/` | `scripts/actr/` | Actr release and repository scripts |
| `.github/`, shared style files | repository root | Monorepo-wide automation baseline |

The old workspace manifest is split after the pure relocation commit: its
package section moves to `crates/actr/Cargo.toml`, while workspace
configuration and shared dependencies remain in the repository-root
`Cargo.toml`. A following build-system commit updates path dependencies.

## Loran naming boundary

The service suite uses Loran consistently in its current tree: ownership paths,
Cargo packages, executables, configuration keys, environment variables, metrics,
deployment assets and test fixtures. Historical Git commits retain their
original names and paths.

- Reusable services live in `crates/loran-*`.
- Deployment, configuration and operations material lives under `loran/`.
- Operational scripts and benchmarks live under `scripts/loran/` and
  `tests/benchmarks/loran/`.

The semantic migration is a breaking change for installed deployments and
external consumers. See [the Loran naming migration guide](loran-naming.md) for
package, configuration, storage and deployment requirements.

## Verification

- The source repository remains byte-for-byte untouched.
- The imported branch contains only verified noreply values or an empty value
  for `root`; messages contain no email address.
- The final tree contains no `.import/actr/` files.
- `git log --follow` crosses the relocation for representative files.
- `cargo metadata --no-deps` resolves the relocated workspace.
- Targeted checks run for the Actr workspace after metadata resolution.

## Final incremental synchronization

The source was checked again on 2026-09-02 before archive preparation. Its
`main` branch had advanced from migration baseline
`61c7c3df751f5c432e7b6b4c169cdbac2842b142` to
`7a6ba0453cb93a45e6198372fc3509b7f52368ed`. Each of the twelve intervening
commits was reviewed against the consolidated tree rather than replayed
blindly:

| Source commit | Consolidated commit | Disposition |
| --- | --- | --- |
| `76b52313` | `dedefa89` | Migrated with the Loran signaling path mapping |
| `87f2a6ee` | `0bdd56c6` | Migrated with the C ABI package path mapping |
| `b19f03ab` | `581f8989` | Migrated and reconciled with the unified Actor package layout |
| `61bc611f` | — | Superseded by the consolidated repository's explicit Wasmtime `46.0.3` security and MSRV policy |
| `812c12c4` | `e23a0797` | Migrated across the language package mappings |
| `9dbd215f` | `ce6e1b78` | Migrated |
| `0897d992` | `dfa7a26e` | Migrated |
| `f9591801` | `f1401b86` | Migrated with consolidated package and tool paths |
| `91e0cbc9` | `81c73346` | Migrated |
| `6e4bfd2f` | — | Not replayed on `main`; the consolidated release preparation remains isolated on its release branch |
| `19ff662a` | `2b8b0fc5` | Migrated |
| `7a6ba045` | `d20cae2c` | Migrated with the repository-root CI workflow mapping |

The migrated commits preserve source author names and author timestamps while
using verified GitHub noreply identities. A destination-only follow-up,
`79417247`, normalizes the generated TypeScript contract terminator so the
checked-in output and generator agree.

The source was frozen again on 2026-09-03. Two further `main` commits were
reviewed and migrated through their consolidated ownership paths:

| Source commit | Consolidated commit | Disposition |
| --- | --- | --- |
| `900d493c` | `dc1dac20` | Migrated from `bindings/kotlin` to `packages/actr-kotlin` |
| `32927127` | `15d65aff` | Migrated from Loran service paths to the corresponding Loran crates and documentation |

The remaining source pull requests are recorded in
[`Actrium/actrium#185`](https://github.com/Actrium/actrium/issues/185). GitHub
cannot transfer pull requests between repositories, so continuing work is
recreated against the monorepo while the archived source pages preserve the
original review and patch history.
