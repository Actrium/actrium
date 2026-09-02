# Migration

The consolidation is expected to proceed in small, reversible stages:

1. Agree on the repository shape and ownership boundaries.
2. Import Actr, Actrium, Keel, and Loran source histories through explicit,
   reviewable migration boundaries.
3. Make the existing builds run from their new paths.
4. Promote shared contracts and consolidate repository tooling.
5. Rename product-facing Actrium surfaces to Actrium and Actrix service surfaces
   to Loran where appropriate.
6. Remove temporary compatibility paths only after downstream users migrate.

This directory holds import maps, compatibility notes, and cutover checklists.
The Actr/Loran source tree, Keel, and Actrium have completed the history-import
stage. Keel and Actrium have also been relocated into the shared monorepo shape.
See the [Actr import plan](actr-import.md), [Keel import record](keel-import.md),
the [Actrium import record](actrium-import.md), and the
[source repository archive checklist](source-archive-readiness.md). Product-facing
renames to Loran and Actrium remain compatibility-aware work under
step 5.
