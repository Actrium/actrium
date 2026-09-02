# Source repository archive readiness

Status: prepared, not yet approved for archive execution.

This checklist covers the legacy `Actrium/actr`, `Actrium/keel`, and
`Actrium/actro` repositories after their histories and current trees were
consolidated into `Actrium/actrium`.

## Snapshot checked on 2026-09-02

| Repository | `main` tip | Open issues | Open pull requests | State |
| --- | --- | ---: | ---: | --- |
| `Actrium/actr` | `19ff662a26da356b5d631a63bb04a18682dee384` | 0 | 14 | Private, active |
| `Actrium/keel` | `296ed6748b8749a0b8de6f78d127ce2f9474805c` | 0 | 0 | Private, active |
| `Actrium/actro` | `755dea237b079b98f364634e83b6e0fdc88fd734` | 0 | 0 | Private, active |

Keel and Actro had no post-migration `main` increment at this snapshot. Actr's
post-migration increment is recorded in [the Actr import plan](actr-import.md).
The counts above cover ordinary issues and pull requests; a final live query is
required immediately before archive execution.

## Required before archiving

- Merge the final synchronization into `Actrium/actrium` and confirm its CI is
  green.
- Re-read all three source `main` tips after that merge. If a tip changed,
  review and synchronize the new range before continuing.
- Resolve Actr's fourteen open pull requests explicitly. A pull request cannot
  be transferred like an issue; merge it, close it with a destination link, or
  retain it as an intentionally archived record.
- Add a concise source-repository notice pointing contributors to
  `Actrium/actrium`, and update repository descriptions if desired.
- Confirm that no release, security, or automation process still writes to a
  source repository.
- Record the final source tips and the consolidated merge commit in this file.
- Obtain an explicit archive instruction before changing repository state.

## Archive execution

Archive only after the checklist above is complete. Archiving makes the source
repository read-only but preserves commit, issue, pull-request, and link
history. Do not delete the repositories: their historical URLs remain useful
for migrated issue and pull-request references.
