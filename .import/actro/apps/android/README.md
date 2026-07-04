# Android

Android platform apps live under this directory.

The imported Actro Android client lives at:

```text
apps/android/actro/
```

Keep Android lifecycle, network monitoring, permissions, native modules, Gradle
configuration, and platform shell behavior in the app unless a shared adapter is
clearly warranted. Shared React Native runtime code belongs in
`packages/actro-runtime/`, and shared non-UI native contracts should flow
through the Actro Rust boundary.
