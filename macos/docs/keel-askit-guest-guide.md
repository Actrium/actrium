# Develop an Actrium Front for macOS

[简体中文](keel-askit-guest-guide.zh.md) · [Front API reference](front-api-reference.md)

An Actrium Front is a React UI written in JS or TS. The Guest runs in a Keel sandbox, and Actrium renders its UI using the macOS Host's registered components. The Guest calls Host features through named `host:*` imports. A Realm Front can also call its bound ACTR Service for business logic. This guide covers the current Actrium v1 JS Front, packaged as a signed `keel.guest/1` workload.

## 1. Write a Guest

Start at the root of an Actrium checkout with Bun and the workspace's JS dependencies installed. Use an `actr` CLI on your PATH that supports `pkg keygen`, `pkg pack`, and `pkg verify`. Use an Actrium macOS build from a compatible checkout for the runtime steps.

```sh
actrium_repo="$PWD"
front_dir="$(mktemp -d "${TMPDIR:-/tmp}/actrium-front.XXXXXX")"
cd "$front_dir"
mkdir -p workload dist
```

Save this as `guest.tsx`. Keel v1 uses the default export as the root component. Import UI components from `keel/guest`. This example increments a counter and requests a clipboard write from the Host.

```tsx
import { useState } from 'react';
import { Button, Text, View } from 'keel/guest';
import { writeText } from 'host:clipboard';

export default function Counter() {
  const [count, setCount] = useState(0);
  const [message, setMessage] = useState('');

  async function copyCount() {
    try {
      await writeText({ text: String(count) });
      setMessage('Copied');
    } catch (error) {
      setMessage(error instanceof Error ? error.message : String(error));
    }
  }

  return (
    <View style={{ padding: 16, gap: 8 }}>
      <Text>Count: {count}</Text>
      <Button title="Increment" onPress={() => setCount((value) => value + 1)} />
      <Button title="Copy count" onPress={copyCount} />
      <Text>{message}</Text>
    </View>
  );
}
```

For an editor or `tsc` project, save the following as `host-modules.d.ts` and include it in your TypeScript project. TypeScript also needs to resolve the local `keel` and React packages. Keel's compiler links `host:*` imports from the Host contract.

```ts
declare module 'host:clipboard' {
  export function writeText(input: { text: string }): Promise<void>;
}
```

## 2. Declare and build the workload

Save the following as `manifest.toml`. The package requests UI rendering and clipboard write. Actrium still prompts for authorization before writing to the clipboard. The packer fills in the file sizes, SHA-256 digests, and signing key ID.

```toml
format = "actr-package/2"

[actor]
publisher = "dev.example"
name = "counter-front"
version = "0.1.0"

[workload]
profile = "keel.guest/1"
entry = "workload/guest.js"

[workload.keel]
runtime = "js"
guest-api = "keel.guest/1"
capabilities = "workload/capabilities.json"

[host]
profile = "actr.host/1"
api = "^1.0.0"
required-capabilities = ["ui.render", "host:clipboard.write"]

[signature]
algorithm = "ed25519"
key-id = "placeholder"

[[files]]
path = "workload/guest.js"
role = "workload"
media-type = "text/javascript"
size = 0
sha256 = "0000000000000000000000000000000000000000000000000000000000000000"

[[files]]
path = "workload/capabilities.json"
role = "profile"
media-type = "application/json"
size = 0
sha256 = "0000000000000000000000000000000000000000000000000000000000000000"
```

Run the v1 build from the example directory. The compiler emits both the linked bundle and its capability manifest. Keep the two files from the same build. The capability manifest records the Guest's imports and components, and `manifest.toml` declares the package's requests.

```sh
bun "$actrium_repo/packages/keel/src/cli/bin.ts" build guest.tsx \
  --contract "$actrium_repo/packages/actrium/src/runtime-host/contract.ts" \
  --v1-capability-manifest workload/capabilities.json \
  --outfile workload/guest.js
```

## 3. Sign and verify the package

Generate a development key once in the example directory and reuse it for rebuilds. The separate public key file is suitable for verification.

```sh
actr pkg keygen --no-register --publisher dev.example \
  --output dev-key.json --public-key-output dev-public-key.json
```

Pack the example and check the signature and contents. The archive contains the declared payload files.

```sh
actr pkg pack --manifest-path manifest.toml --payload-root . \
  --key dev-key.json --output dist/counter-front.actr
actr pkg verify --package dist/counter-front.actr --pubkey dev-public-key.json
```

## 4. Open and rebuild on macOS

1. Add the **public** key from `dev-public-key.json` to the local Actrium trust store at `~/Library/Application Support/Actrium/trusted-keys.json`. It is a JSON array of objects shaped like `{"public_key":"<base64 public key>"}`. Append an entry if the file exists. The CLI verification step and Actrium's local trust store are separate.
2. In the Actrium address bar, paste the absolute path printed by `printf '%s\n' "$PWD/dist/counter-front.actr"`.
3. Click **Increment** and verify that the count changes. Click **Copy count**, grant clipboard access when prompted, then paste in a text field to verify the result. A denied call is shown in the Guest's status text.
4. Change the label in `guest.tsx`, rerun the build, pack, and verify commands with the same key and output path, then reload the Tab with **⌘R**. If the Tab points at an installed copy, reopen the rebuilt archive by pasting its absolute file path.

See the [package manifest guide](manifest-specification.md) for resources and metadata.

## 5. Call a Realm Service

For a Service-backed Front, declare the Service package and its Proto contract under `[dependencies.service]`, request `service.call`, and import the exact RPC route. The following fragment uses the encoder and decoder from Echo Chat.

```ts
import { call as echo } from 'host:service/echo.v1.EchoService.Echo';
import { decodeEchoReply, encodeEchoRequest } from './echo-wire';

async function requestEcho(message: string) {
  const reply = await echo({
    payloadBase64: encodeEchoRequest(message),
    timeoutMs: 8_000,
  });
  return decodeEchoReply(reply.payloadBase64);
}
```

Open the Front in the Realm that selects the matching Service. The clipboard example works when opened directly from the local archive. Service calls require a Realm-bound Tab. Follow [Echo Chat](https://github.com/Actrium/actr-hub/tree/main/echo) for the complete source, package, and Service launcher, and [Develop a Front locally](https://github.com/Actrium/actr-hub/blob/main/docs/develop-front-locally.md) for using a local build in a Realm.

## Troubleshooting

| Symptom | Check |
| --- | --- |
| `component_not_allowed` | Confirm the import appears in the Actrium [component list](front-api-reference.md#components). |
| `capability_mismatch` or a missing capability manifest | Rebuild the bundle and its capability manifest together. Check the package's declared capability and the `[workload.keel]` manifest path. |
| Clipboard call denied | Confirm `host:clipboard.write` is declared and that the user granted the request in Actrium. |
| Service call or subscription is unavailable | Check the fixed route or event, the Proto contract, the running Service, and the Realm's Front–Service binding. |
| Archive verification or loading fails | Run `actr pkg verify` using the matching public key, then check that Actrium trusts that key and that the Tab points at the rebuilt archive. |

The [Front API reference](front-api-reference.md) lists the supported Host methods, UI components, and Guest runtime APIs.
