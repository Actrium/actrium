# 0001: Organize the monorepo by product layers

Status: proposed

## Context

Actr, Actro, and Keel currently live in separate repositories, although the
browser increasingly composes all three as one product. A merged repository
needs to expose that relationship without erasing subsystem boundaries.

## Decision

Use three visible implementation roots: **engine/**, **browser/**, and
**ui/keel/**. Put cross-layer schemas in **contracts/**, and reserve the
repository root for workflows that genuinely span the combined product.

## Consequences

- The repository reads from foundation to product without requiring knowledge
  of the former repository names.
- Actr can still be built and released independently as the reusable engine.
- Keel's host/guest security boundary remains visible.
- Initial imports can preserve package histories and be flattened gradually.
- Root build orchestration will need to coordinate multiple language toolchains.
