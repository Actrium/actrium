# Actrium

Actrium is the private monorepo for the Actrium browser applications, the Actr
foundation, the Keel sandbox UI framework, and Loran—the default shore-side
service suite.

Actr, its former Loran service suite, Keel, and Actrium have been migrated into
this repository through reviewed, privacy-sanitized history imports. The Actrium
applications now occupy the platform roots while their reusable code lives in
`crates/` and `packages/`.

## Project shape

    actrium/
    ├── actr/            Actr's public boundary and integration metadata
    ├── keel/            Keel's public boundary and integration metadata
    ├── loran/           Default/reference shore-side service composition
    ├── android/         Actrium Android application
    ├── ios/             Actrium iOS application
    ├── macos/           Actrium macOS application
    ├── windows/         Actrium Windows application
    ├── crates/          Reusable Rust crates, including Actr and Loran
    ├── packages/        Actr bindings, Keel, and other non-Rust packages
    ├── contracts/       Cross-language and cross-process protocol boundaries
    ├── examples/        Focused examples for each component
    ├── tests/           Cross-component conformance, integration, and e2e tests
    ├── docs/            Architecture, decisions, migration notes, and product docs
    ├── rfcs/            Design proposals spanning the combined project
    ├── scripts/         Repository workflows
    └── tools/           Repository-local developer tools

The named roots are the visible components. `crates/` and `packages/` are the
single, typed entrances for their implementation units; they are not another
layer in the product hierarchy.

The intended dependency direction is:

    android / ios / macos / windows ──► keel ──► actr
                 │                       │         ▲
                 └──────► packages + crates ───────┘
    loran ───────────────────────────────► actr

`contracts/` defines the boundaries between those components. It is not a
general dumping ground for shared utilities.

See [the repository layout](docs/architecture/repository-layout.md) for the
proposed source mapping and the reasoning behind it.

## Scope

- Actr is the built-in foundation rather than a sibling product.
- Actrium evolves into the Actrium applications and their shared packages.
- Keel remains a distinct sandbox UI component, while shipping with Actrium.
- Loran is the default and reference shore-side service suite; it includes the
  operational composition rather than becoming part of Actr itself.
- Supramark remains an independent project. Its implementation and dependencies
  are not part of this monorepo.

No license has been selected for the combined repository yet.
