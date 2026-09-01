# macOS

macOS platform apps live under this directory.

The imported Actro macOS client lives at:

```text
apps/macos/actro/
```

Keep AppKit/SwiftUI lifecycle, WebKit shell behavior, XPC services, CocoaPods/
Xcode setup, and desktop-specific integrations in the app unless a shared Apple
package is clearly warranted. Shared Apple code should move into
`platform/apple/packages/`, shared React Native runtime code belongs in
`packages/actro-runtime/`, and shared non-UI native contracts should flow
through the Actro Rust boundary.
