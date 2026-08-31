# Actrium

Actrium is the private monorepo for the Actrium browser, its embedded Actr
engine, and the Keel sandbox UI framework.

This first revision is deliberately a **layout prototype**. It contains no
migrated implementation code yet; its purpose is to make the combined project
shape concrete before repository histories, build systems, and packages move.

## Project shape

    actrium/
    ├── engine/          Actr runtime, gateway, SDK bindings, and CLI
    ├── browser/         Actrium product, platform applications, and browser model
    ├── ui/keel/         Sandboxed dynamic UI framework embedded by the browser
    ├── contracts/       Cross-language and cross-layer protocol boundaries
    ├── examples/        Focused examples for each major layer
    ├── tests/           Cross-layer conformance, integration, and end-to-end tests
    ├── docs/            Architecture, decisions, migration notes, and product docs
    ├── rfcs/            Design proposals spanning the combined project
    ├── scripts/         Repository workflows
    └── tools/           Repository-local developer tools

The intended dependency direction is:

    browser ───────► ui/keel
       │                │
       └────────┬───────┘
                ▼
              engine

The **contracts/** directory defines the boundaries between those layers. It is
not a general dumping ground for shared utilities.

See [the repository layout](docs/architecture/repository-layout.md) for the
proposed source mapping and the reasoning behind it.

## Scope

- Actr becomes the built-in engine rather than a sibling product.
- Actro evolves into the Actrium browser layer.
- Keel becomes the browser's sandbox UI framework while retaining a clear
  internal boundary.
- Supramark remains an independent project; Actrium may integrate it through a
  narrow adapter later, but its implementation is not part of this monorepo.

No license has been selected for the combined repository yet.
