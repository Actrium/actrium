# Third-party notices

Third-party code retains its original licenses and copyright notices. The
repository's Apache-2.0 license applies to first-party code unless otherwise
noted; it does not replace the terms of third-party components.

This document records the vendored React Native and QuickJS sources listed
below. Other dependencies and components retain the licenses and notices
included in their distributions.

## React Native

Source: [React Native](https://github.com/facebook/react-native).

Copyright (c) Meta Platforms, Inc. and affiliates.

The following React Native sources and adaptations retain the
[MIT license](packages/keel/native/jsi/LICENSE):

- `packages/keel/native/jsi/`
- `packages/keel/native/jsc/src/JSCRuntime.h`
- `packages/keel/native/jsc/src/JSCRuntime.mm`
- `packages/actrium/modify_modules/react-native/`
- `examples/keel/android-demo/android/app/src/main/jni/OnLoad.cpp`
- The React Native additions to `android/android/gradlew.bat`; the Gradle
  wrapper's Apache-2.0 notice also remains in that file.

Upstream file headers referring to an MIT `LICENSE` at the source-tree root
refer to the linked React Native MIT license, not Actrium's root
Apache-2.0 license. Keel includes the full MIT text in its npm package.

## QuickJS

Source: [QuickJS](https://bellard.org/quickjs/).

The sources in `packages/keel/native/quickjs/vendor/` retain their original
copyright notices and [MIT license](packages/keel/native/quickjs/vendor/LICENSE).
Keel includes that license text in its npm package.
