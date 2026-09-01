# Actr import plan

Status: accepted for the initial Actr migration.

## Goals

- Preserve the useful `main` history while excluding obsolete path-only
  history, old branches, and release tags.
- Preserve author and committer names, timestamps, messages, and parent
  relationships for retained commits.
- Remove email values from commit identities and redact email addresses in
  commit messages. Source files are not rewritten merely because they contain
  email-shaped test data or package metadata.
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
4. Clear `author_email` and `committer_email`, redact email addresses in commit
   messages, and remove invalidated commit signatures.
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
| `actrix/crates/<name>/` | `crates/loran-<name>/` | Directory ownership changes now; public crate renames follow separately |
| `actrix/deploy/` | `loran/deploy/` | Deployable service composition |
| `actrix/docs/` | `loran/docs/` | Operations and service documentation |
| `actrix/config.example.toml` | `loran/config/actrix.example.toml` | Compatibility name retained during import |
| `actrix/benchmarks/` | `tests/benchmarks/loran/` | Service-suite performance checks |
| `actrix/scripts/` | `scripts/loran/` | Loran operational workflows |
| `testing/*/` | `crates/<package-name>/` | Buildable Rust test support crates |
| `tools/` | `tools/actr/` | Actr generators, linters, and developer tools |
| `examples/` | `examples/actr/` | Component examples |
| `e2e/` | `tests/e2e/actr/` | Actr end-to-end suites |
| `experiments/` | `examples/actr/experimental/` | Clearly non-reference exploratory code |
| `docs/` | `docs/actr/` | Actr-specific implementation documentation |
| `rfcs/` | `rfcs/actr/` | Existing Actr proposals |
| `scripts/` | `scripts/actr/` | Actr release and repository scripts |
| `docker/runner/` | `tools/ci/runner/` | Repository CI runner support, not Loran deployment |
| `.github/`, shared style files | repository root | Monorepo-wide automation baseline |

The old workspace manifest is split during relocation: its package section
moves to `crates/actr/Cargo.toml`, while workspace configuration and shared
dependencies remain in the repository-root `Cargo.toml`. Path dependencies
are updated in the same relocation commit.

## Verification

- The source repository remains byte-for-byte untouched.
- The imported branch contains no real email value in commit or annotated-tag
  metadata or messages.
- The final tree contains no `.import/actr/` files.
- `git log --follow` crosses the relocation for representative files.
- `cargo metadata --no-deps` resolves the relocated workspace.
- Targeted checks run for the Actr workspace after metadata resolution.
