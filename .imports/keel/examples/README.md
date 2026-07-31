# Keel Examples

Example applications demonstrating the keel SDK across different platforms.

## Examples

| Example | Platform | Status | Description |
|---------|----------|--------|-------------|
| [ios-demo](./ios-demo/) | iOS | Ready | Full iOS app with performance dashboard, 3 sandbox engines (JSC/Hermes/QuickJS), Bridgeless only |
| [android-demo](./android-demo/) | Android | Ready | Android app with Hermes and QuickJS sandboxes plus emulator E2E runner |
| [macos-demo](./macos-demo/) | macOS | Placeholder | macOS app via `react-native-macos` |
| [windows-demo](./windows-demo/) | Windows | Placeholder | Windows app via `react-native-windows` |

## Dependency Resolution

Examples reference `keel` via relative `file:` paths, not npm registry:

| Example | keel dependency |
|---------|----------------|
| `ios-demo/RCTNApps` | `"keel": "file:../../../"` |

This ensures examples always use the local checkout of keel.

## Package Exports Reference

Guest bundles import from `keel/guest`:

```tsx
import { View, Text, useHostEvent, useSendToHost, useConfig } from 'keel/guest';
```

Host apps import from `keel/host` and `keel/host/preset`:

```tsx
import { Engine } from 'keel/host';
import { DefaultComponents, EngineView } from 'keel/host/preset';
```
