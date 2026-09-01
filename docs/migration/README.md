# Migration

The consolidation is expected to proceed in small, reversible stages:

1. Agree on the repository shape and ownership boundaries.
2. Import Actr, Actro, Keel, and Loran source histories through explicit,
   reviewable migration boundaries.
3. Make the existing builds run from their new paths.
4. Promote shared contracts and consolidate repository tooling.
5. Rename product-facing Actro surfaces to Actrium and Actrix service surfaces
   to Loran where appropriate.
6. Remove temporary compatibility paths only after downstream users migrate.

This directory holds import maps, compatibility notes, and cutover checklists.
The Actr/Loran source tree, Keel, and Actro have completed the history-import
stage. Keel and Actro have also been relocated into the shared monorepo shape.
See the [Actr import plan](actr-import.md), [Keel import record](keel-import.md),
and [Actro import record](actro-import.md). The Actrix-to-Loran and
Actro-to-Actrium product-facing renames remain compatibility-aware work under
step 5.
