# Source repository archive readiness

Status: archive execution authorized after the final synchronization merges.

This checklist covers the legacy `Actrium/actr`, `Actrium/keel`, and
`Actrium/actro` repositories after their histories and current trees were
consolidated into `Actrium/actrium`.

## Snapshot checked on 2026-09-03

| Repository | `main` tip | Open issues | Open pull requests | State |
| --- | --- | ---: | ---: | --- |
| `Actrium/actr` | `329271278629ec28705a1affd1535dee368ade16` | 0 | 15 | Private, active |
| `Actrium/keel` | `296ed6748b8749a0b8de6f78d127ce2f9474805c` | 0 | 0 | Private, active |
| `Actrium/actro` | `755dea237b079b98f364634e83b6e0fdc88fd734` | 0 | 0 | Private, active |

Keel and Actro had no post-migration `main` increment at this snapshot. Actr's
post-migration increments are recorded in
[the Actr import plan](actr-import.md). Actr's last ordinary issue was
transferred to `Actrium/actrium#184`; its remaining pull requests are
classified in `Actrium/actrium#185`. The counts above cover ordinary issues
and pull requests; a final live query is required immediately before archive
execution.

## Required before archiving

- Merge the final synchronization into `Actrium/actrium` and confirm its CI is
  green.
- Re-read all three source `main` tips after that merge. If a tip changed,
  review and synchronize the new range before continuing.
- Confirm transferred Actr issue `#667` continues to resolve to
  `Actrium/actrium#184`.
- Close Actr's fifteen remaining pull requests with a link to the canonical
  disposition record, `Actrium/actrium#185`. Pull requests cannot be
  transferred like issues; their archived pages retain the original review,
  commit, and patch history.
- Add a concise source-repository notice pointing contributors to
  `Actrium/actrium`, and update repository descriptions if desired.
- Confirm that no release, security, or automation process still writes to a
  source repository.
- Record the final source tips and the consolidated merge commit in this file.
- Retain the explicit 2026-09-03 archive instruction with the migration task
  record.

## Archive execution

Archive only after the checklist above is complete. Archiving makes the source
repository read-only but preserves commit, issue, pull-request, and link
history. Do not delete the repositories: their historical URLs remain useful
for migrated issue and pull-request references.
