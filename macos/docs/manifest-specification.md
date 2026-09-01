# Actrium Actor Package manifest

Actrium does not define a separate application archive. It consumes the shared
`actr-package/1` `.actr` format and requires a `keel.guest/1` workload.

Every package contains a root `manifest.toml`, a raw Ed25519 `manifest.sig`, and
only payloads declared by `[[files]]`. A minimal Actrium Guest manifest is:

```toml
format = "actr-package/1"

[actor]
manufacturer = "acme"
name = "notes"
version = "1.0.0"

[workload]
profile = "keel.guest/1"
entry = "workload/guest.js"

[workload.keel]
runtime = "js"
guest-api = "keel.guest/1"

[host]
profile = "actr.host/1"
api = "^1.0.0"
required-capabilities = ["ui.render"]
optional-capabilities = []

[extensions.actrium]
title = "Notes"
description = "A Keel Guest notes application"
left-panel-default-visible = false
right-panel-default-visible = false

[signature]
algorithm = "ed25519"
key-id = "placeholder"

[[files]]
path = "workload/guest.js"
role = "workload"
media-type = "text/javascript"
size = 0
sha256 = "0000000000000000000000000000000000000000000000000000000000000000"
```

The packer replaces `size`, `sha256`, and `signature.key-id` with canonical
values before signing. Actrium-specific presentation metadata belongs under
`[extensions.actrium]`; it does not change Actor identity, workload semantics,
Host requirements, or package trust.

The normative field, path, signature, and loading rules are defined in
[`actr/protocols/actor-package.zh.md`](../../actr/protocols/actor-package.zh.md).
Use `cargo run -p actr-cli -- pkg pack --help` to build a package and
`cargo run -p actr-cli -- pkg verify --help` to verify one.
