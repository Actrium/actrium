# Migration

The consolidation is expected to proceed in small, reversible stages:

1. Agree on the repository shape and ownership boundaries.
2. Import Actr, Actro, Keel, and the future Loran service history without
   rewriting their internals.
3. Make the existing builds run from their new paths.
4. Promote shared contracts and consolidate repository tooling.
5. Rename product-facing Actro surfaces to Actrium and Actrix service surfaces
   to Loran where appropriate.
6. Remove temporary compatibility paths only after downstream users migrate.

This directory will hold import maps, compatibility notes, and cutover
checklists. The current commit completes only step 1 as a prototype.
