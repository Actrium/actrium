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
The Actr history and current Actr/Loran source tree have completed steps 2 and
3. Actro and Keel have not yet been imported; the Actrix-to-Loran semantic
rename remains part of step 5.
