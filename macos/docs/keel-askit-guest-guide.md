# Keel Guest development for Actrium

A UI running in Actrium is still an Actor workload. It is built as one
`keel.guest/1` workload and distributed as the same signed `.actr` package used
by every other Actor workload profile.

## Responsibilities

- Keel serializes Guest React operations and renders them through the Host's
  registered native components.
- The Actrium app SDK exposes capability-scoped Host/Guest APIs.
- The `.actr` manifest declares Actor identity, workload entry, Host API range,
  requested capabilities, every payload digest, and publisher signature.
- Actrium verifies the package before creating the sandbox and does not execute
  undeclared files.

## Project shape

```text
my-guest/
├── manifest.toml
├── src/
│   └── app.tsx
└── workload/
    └── guest.js
```

The source entry exports the Guest application contract expected by Keel. The
Counter example exports `usePanels`; the Echo example demonstrates Actor RPC.

```tsx
import React, { useState } from 'react';
import { Text, TouchableOpacity, View } from 'react-native';

export function App() {
  const [count, setCount] = useState(0);
  return (
    <View style={{ padding: 16 }}>
      <Text>Count: {count}</Text>
      <TouchableOpacity onPress={() => setCount((value) => value + 1)}>
        <Text>Increment</Text>
      </TouchableOpacity>
    </View>
  );
}
```

## Build and package

Use Keel's build command with the Actrium app SDK Guest footer:

```bash
bun packages/keel/src/cli/bin.ts build src/app.tsx \
  -o workload/guest.js \
  --footer packages/actrium-app-sdk/src/cli/keel-guest-footer.js
```

Then package and verify through the shared Rust implementation:

```bash
cargo run -p actr-cli -- pkg pack \
  --manifest-path manifest.toml \
  --payload-root . \
  --key path/to/development-key.json \
  --output dist/my-guest.actr

cargo run -p actr-cli -- pkg verify \
  --package dist/my-guest.actr \
  --pubkey path/to/development-key.json
```

Do not create the archive with a generic ZIP command: the packer canonicalizes
file metadata, updates payload digests and key identity, enforces ZIP `STORE`,
and signs the exact manifest bytes.

## Sandbox boundary

Guest code may use React, Keel Guest APIs, and Host capabilities explicitly
granted to the package. It may not use Node.js file APIs, browser globals,
native modules, ambient network access, or dynamic code loading. New Host
capabilities require a versioned contract plus an implementation and policy in
the Host.

Runnable examples:

- [`packages/actrium/keel-guest-examples/counterapp`](../../packages/actrium/keel-guest-examples/counterapp)
- [`examples/actrium/echo`](../../examples/actrium/echo)

See the [Actor Package manifest](manifest-specification.md) and normative
[Actor Package specification](../../actr/protocols/actor-package.zh.md).
