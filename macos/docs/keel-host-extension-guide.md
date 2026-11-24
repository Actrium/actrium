# Extend the Actrium Keel Host on macOS

[简体中文](keel-host-extension-guide.zh.md) · [Front development guide](keel-askit-guest-guide.md) · [Front API reference](front-api-reference.md)

This guide is for maintainers exposing an interface to v1 JS Fronts on macOS. The compiler records what a Guest imports in `workload/capabilities.json`. The Host reconstructs that list from the bundle linkage before loading it and binds the verified interfaces. Each callable capability also has a package declaration and a runtime authorization policy. [Bundle and Host verification](../../packages/actrium/src/runtime-host/v1-guest.ts)

| What the Guest needs | Extension type | How the Guest uses it |
| --- | --- | --- |
| A Host operation with a result | [Host RPC](#host-rpc) | Named `host:*` import, returns a Promise |
| Updates from an ongoing Host operation | [Host subscription](#host-subscriptions) | Named `host:*` import, returns an unsubscribe function |
| A business call or event from the bound ACTR Service | [Service route or event](#service-routes-and-transports) | Named `call` or `on` import from `host:service/<name>` |
| An element in its React tree | [UI component](#ui-components) | Named component import from `keel/guest` |
| A sandbox-local helper, runtime module, or global | [Guest SDK and runtime](#guest-sdk-and-runtime) | SDK or module import, or an explicitly provided sandbox global |

## Host RPC

Use the existing clipboard write as a worked example. Its Guest module is `host:clipboard`, its method is `writeText`, and the package declares the capability ID `host:clipboard.write`.

1. **Define the boundary.** Add an `rpc<Input, Output>()` descriptor under the module in `contract.hostModules`. Validate the Guest's input and the returned data where applicable. Clipboard write accepts `{ text: string }` and returns no value. [Host contract](../../packages/actrium/src/runtime-host/contract.ts)
2. **Map the capability and set policy.** Add or update its entry in `FRONT_CAPABILITY_SPECS` to map the ID to the Keel module and method. In `FRONT_CAPABILITY_DEFINITIONS`, set the audience, grant requirement and, where applicable, platform permission or broker dependency. Review module shorthand expansion when adding a method to an existing module. Use exact method IDs in new Guest packages. [Capability IDs and shorthands](../../packages/actrium/src/runtime-host/capabilities.ts) · [Authorization policy](../../packages/actrium/src/runtime-host/capability-model.ts)
3. **Implement and authorize.** Add the method to `createHostModules`. Check authorization before performing the operation. For clipboard write, the method awaits `authorizeCapability('host:clipboard.write')` before calling the macOS clipboard adapter. Wire platform dependencies through the Host. For features owned outside the renderer, follow the file picker pattern and check the active Tab, authorization, and arguments again where the native operation runs. [Host implementations](../../packages/actrium/src/runtime-host/host-modules.ts) · [macOS wiring](../../packages/actrium/src/runtime-host/StandaloneGuestApp.tsx) · [Native file picker](../Actrium/MultiProcess/RendererFileImport.swift)
4. **Open the method.** Add the implemented export to `HOST_SUPPORTED_METHODS` and set `implemented: true` in its capability definition. The supported module set is derived from the method list. The effective authorization set uses the capability definition. Keep both gates aligned. [Method allowlist](../../packages/actrium/src/runtime-host/host-modules.ts) · [Effective capability definitions](../../packages/actrium/src/runtime-host/capability-model.ts)
5. **Verify as a Guest.** Add a typed declaration for the import, make the test Front request the exact ID under `[host]`, and build with the Actrium contract. Check the compiled manifest and invoke the method from the Guest. Test a missing declaration, invalid input, and denied access. A rejection should leave the native operation untouched. [Guest build and package steps](keel-askit-guest-guide.md) · [Host method tests](../../packages/actrium/src/runtime-host/host-modules.test.ts)

Call this helper from a Guest event handler. With a correctly declared test package, it exercises the same Host path as the public clipboard API.

```ts
import { writeText } from 'host:clipboard';

async function copyLabel(text: string) {
  await writeText({ text });
}
```

## Host subscriptions

Follow the RPC steps for the module export, capability ID, authorization policy, and method allowlist. Use `subscription<Event>()` in the contract and validate the event payload with `parseEvent`. The compiler lists the export under `events`. Give the Guest a synchronous unsubscribe function, and tie cleanup to the lifetime of its Engine or Tab. For protected sources, authorize before attaching the source. If authorization can be revoked, stop delivery and release the subscription when it is revoked.

`host:speech.onInterimResult` is an example of an event tied to an authorized `recognize` call for the same Guest. `service.on` shows how to cancel pending authorization and clean up a long-lived transport. Test two Guests, unsubscribe, Tab teardown, and revocation where supported. [Subscription contracts](../../packages/actrium/src/runtime-host/contract.ts) · [Subscription implementations](../../packages/actrium/src/runtime-host/host-modules.ts) · [Subscription tests](../../packages/actrium/src/runtime-host/host-modules.test.ts)

## Service routes and transports

A new business RPC or event uses the existing `service.call` or `service.on` mechanism. Update the Service and its Proto or event contract, link the exact name in the Front, and verify access to the bound Service. The Host pins the name from the import and filters subscriptions by that name. Adding a business route uses the existing Host capability IDs. [Front Service example](keel-askit-guest-guide.md#5-call-a-realm-service) · [Fixed-route binding](../../packages/actrium/src/runtime-host/v1-guest.ts)

A new Host-owned transport operation is an API change. For an operation exposed directly by `host:service` like `uploadAttachment`, follow the Host API steps above and update both `V1_SERVICE_TRANSPORTS` in the linker and `SERVICE_TRANSPORT_CAPABILITIES` in the Host manifest verifier. Check whether it needs an exact-only capability declaration. Test the tab-specific availability check and confirm that packages missing the exact capability are rejected when exact declaration is required. [v1 linking rules](../../packages/keel/src/v1.ts) · [Service manifest verifier](../../packages/actrium/src/runtime-host/capability-verify.ts) · [Transport linkage tests](../../packages/actrium/src/runtime-host/v1-guest.test.ts)

## UI components

A component is exposed through its SDK export, the v1 component allowlist, and the macOS component registry. Keep the same name at each step. For an existing protocol alias such as `Image` → `ImageAsset`, follow the explicit mapping in the linker and registration code.

1. **Define Guest props and callbacks.** Declare a typed component in `sdk.ts` using `declareHostComponent<Props>('Name')` and export the component and any public types from `sdk/index.ts`. Expose only the props and event data the Guest needs. [Guest SDK implementation](../../packages/keel/src/sdk/sdk.ts) · [SDK exports](../../packages/keel/src/sdk/index.ts)
2. **Allow the component.** Add its manifest name to `V1_COMPONENTS`. Actrium's `COMPONENT_WHITELIST` uses the same list. The v1 build records the component based on the named import. [Component allowlist](../../packages/keel/src/v1.ts) · [Host policy](../../packages/actrium/src/runtime-host/capability-model.ts)
3. **Register the implementation.** Wire the renderer into `StandaloneGuestApp`'s registry. For v1 Guests, registration is filtered by the verified manifest. If the renderer depends on an optional native module or build flag, check availability before loading a Guest that links it. `Supramark` is an example of a build-dependent renderer. [macOS registry and availability checks](../../packages/actrium/src/runtime-host/StandaloneGuestApp.tsx)
4. **Validate the props and events.** Exercise rendering and callback delivery in a v1 Guest. Test invalid props at the wrapper or native adapter where validation belongs, and confirm unlinked components are unavailable. Use `Image` as a reference for a narrow prop allowlist and a Host-side asset check. [Image prop filter](../../packages/actrium/src/runtime-host/image-asset.ts) · [Image tests](../../packages/actrium/src/runtime-host/image-asset.test.ts) · [Scoped Engine tests](../../packages/actrium/src/runtime-host/v1-guest.test.ts)

### Components backed by native UI

`Select` and `Menu` are UI components. The renderer turns a press on the component into a request to the macOS menu adapter. The native handler checks the Tab, reload key, visible surface, and the component's presence in the manifest. It returns a selected value to the component's callback. For this kind of component, also test dismissal, rejection when the native bridge is unavailable, and stale replies after the Tab changes or the component unmounts. [Guest menu components](../../packages/actrium/src/runtime-host/guest-menu.tsx) · [macOS menu bridge](../ActriumViewService/GuestMenuModule.swift) · [Native menu handler](../Actrium/MultiProcess/RendererGuestMenu.swift)

This Guest can exercise `Select` in a macOS build with the native menu bridge.

```tsx
import { useState } from 'react';
import { Select } from 'keel/guest';

export default function App() {
  const [value, setValue] = useState('small');
  return (
    <Select
      options={[
        { value: 'small', label: 'Small' },
        { value: 'large', label: 'Large' },
      ]}
      value={value}
      onValueChange={setValue}
    />
  );
}
```

## Guest SDK and runtime

- **SDK helper.** Implement and export the helper from `keel/guest`, then add the named export to `V1_SDK_UTILITIES`. The sandbox Guest bundle provides these exports. Test the helper inside the sandbox. The UTF-8 and Base64 helpers are examples. [SDK exports](../../packages/keel/src/sdk/index.ts) · [v1 SDK allowlist](../../packages/keel/src/v1.ts) · [Guest bundle](../../packages/keel/src/guest/bundle.ts) · [Byte helper tests](../../packages/keel/src/sdk/sandbox-utils-bytes.test.ts)
- **Runtime module.** For a new external module like the React JSX runtime, align `V1_RUNTIME_IMPORTS`, the compiler's external-to-global mapping, the actual sandbox provider for that value, and the Host import verifier. Test that the import builds and resolves in a scoped Engine. [v1 imports](../../packages/keel/src/v1.ts) · [Compiler externals](../../packages/keel/src/cli/build.ts) · [Host import verifier](../../packages/actrium/src/runtime-host/runtime-capability-verify.ts)
- **Sandbox global.** Add application-facing globals at the Engine or Guest runtime initialization point that owns them. Test their behavior on applicable sandbox engines and include lifecycle tests where appropriate. Timers and the console are examples. Operations that access OS resources or application data belong behind the Host capability boundary. [Engine global setup](../../packages/keel/src/host/engine/engine.ts) · [Guest console setup](../../packages/keel/src/guest/runtime/globals-setup.ts)

## Verify the extension

Use the existing [v1 compiler tests](../../packages/keel/src/cli/build.v1.test.ts), [Host manifest tests](../../packages/actrium/src/runtime-host/runtime-capability-verify.test.ts), and [scoped Engine tests](../../packages/actrium/src/runtime-host/v1-guest.test.ts) as templates. For a macOS-native operation or UI component, rebuild the system bundle with `./scripts/bundle-actrium-runtime.sh --target system --dev` from `macos/`, then exercise the result in a compatible Actrium build. Verify success and the relevant rejection or cleanup paths.

When a new surface is ready to use, update both editions of the [Front API reference](front-api-reference.md) with its import, contract or props, availability, and an example.
