# Counter Keel Guest

Counter is a small reusable `keel.guest/1` workload for exercising Actrium's
Actor Host. It is distributed only as an `actr-package/1` `.actr` archive.

From this directory:

```bash
bun install
bun run build
bun run build:package
bun run verify:package
```

The signed package is written to `dist/actrium-counter-1.0.0.actr`. The checked
development key is shared with the Echo example and must not be used for a
production release.

Package shape:

```text
actrium-counter-1.0.0.actr
├── manifest.toml
├── manifest.sig
└── workload/
    └── guest.js
```

The normative package model is documented in
[`actr/protocols/actor-package.zh.md`](../../../../actr/protocols/actor-package.zh.md).
