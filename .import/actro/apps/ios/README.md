# iOS

iOS platform apps live under this directory.

The imported Actro iOS client lives at:

```text
apps/ios/actro/
```

Keep UIKit/SwiftUI lifecycle, account UX, permissions, CocoaPods/Xcode setup,
and platform shell behavior in the app unless a shared Apple package is clearly
warranted. Shared React Native runtime code belongs in
`packages/actro-runtime/`, and shared non-UI native contracts should flow
through the Actro Rust boundary.
