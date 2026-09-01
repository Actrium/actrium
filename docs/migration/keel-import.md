# Keel import record

Status: completed on 2026-09-01.

## History import

The source was the `main` branch of `Actrium/keel` at `296ed67`. The import
used the same privacy and history policy as the Actr import:

- preserve every path in the final source tree and its rename/copy
  predecessors;
- discard commits that become empty after obsolete paths are removed, and
  prune degenerate merges;
- preserve contributor names while replacing author and committer email
  values with each contributor's verified GitHub noreply address;
- redact email addresses from commit messages and remove invalidated commit
  signatures;
- import through a temporary prefix, then relocate the current snapshot in a
  separate commit so historical snapshots continue to describe the source
  repository truthfully.

Of 524 source commits, 452 remain in the filtered history. The other 72 were
empty, obsolete-path-only, or degenerate after filtering. The filtered current
tree matched the source current tree exactly before relocation. The import
merge is `d317bfd3`; the relocation commit is `95dc2f0c`.

## Current-tree ownership

| Monorepo path | Responsibility |
| --- | --- |
| `packages/keel/` | Keel TypeScript runtime, native host implementations, scripts, and package tests |
| `keel/` | Keel component documentation, policy, release notes, and documentation build |
| `contracts/sandbox/` | Sandbox wire contracts shared across implementations |
| `crates/keel-native-guest/` | Reusable Rust native-guest SDK |
| `examples/keel/guests/rust/` | Independent Rust/Wasm guest examples and build workspace |
| `examples/keel/*-demo/` | Android, iOS, macOS, and Windows integration examples |

The repository-level Keel workflows live in `.github/workflows/`. The docs
workflow builds the Keel site but does not deploy it until the monorepo has a
single Pages ownership plan.

## Issue migration

All 45 ordinary source Issues were transferred after synchronizing the 12
source labels. GitHub preserved titles, open/closed state, authors, timestamps,
comment counts, labels, and assignees. It rewrote some cross-Issue links while
transferring; old Issue URLs resolve to the new locations.

| Old Keel Issues | New Actrium Issues |
| --- | --- |
| 3, 5, 8, 10, 17, 19, 29 | 134, 135, 136, 137, 138, 139, 140 |
| 39, 40, 41, 42, 43, 44, 45 | 141, 142, 143, 144, 145, 146, 147 |
| 46, 47, 54, 57, 58, 63, 64 | 148, 149, 150, 151, 152, 153, 154 |
| 65, 66, 67, 68, 69, 77, 79 | 155, 156, 157, 158, 159, 160, 161 |
| 80, 82, 84, 85, 90, 95, 100 | 162, 163, 164, 165, 166, 167, 168 |
| 101, 103, 104, 105, 106, 107, 108 | 169, 170, 171, 172, 173, 174, 175 |
| 121, 122, 123 | 176, 177, 133 |

Current-tree references to those Issues use the new Actrium numbers. Numbered
work items local to a document, such as the deferred native-guest plan, remain
unchanged.

GitHub cannot transfer Pull Requests between repositories. The 79 closed Keel
PRs therefore remain in the private, unarchived source repository, preserving
their reviews and links. The source's `0.1.0` prerelease/tag, Pages deployments,
and environment also remain there; branches and tags were not designated as
critical migration material.

## Verification

- The repository-wide identity gate passes all 454 commits introduced by the
  import merge and relocation.
- The final tree contains no temporary import-prefix files.
- Keel TypeScript checks, script tests, unit tests, guest bundle checks, docs
  build, Rust checks/tests/clippy/builds, native C++ suites, and the Windows
  demo typecheck pass from their relocated paths.
- The target repository remains private.
