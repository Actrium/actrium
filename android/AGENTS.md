# Actrium Android Client

## Project overview

Actrium Android Yes. Actrium Browser Client Android Shell, adopt **Kotlin Raw shells + React Native Operational level** The first one is a hybrid. The original shell is more responsible. Tab Window management and system-level capacity,RN Layer (Class)`packages/actrium`) All operations carried UI And logic.

## Warehouse structure

```
android/
├── android/                  # Android Raw shells (Kotlin)
│   ├── app/                  #   Main application module
│   │   └── src/main/java/com/actrium/android/
│   │       ├── MainActivity.kt        # Entry Activity- It's a lot of management. Tab RN Containers
│   │       ├── MainApplication.kt     # Application, Register RN Package
│   │       ├── MainViewModel.kt       # Tab Life cycle ViewModel
│   │       └── core/
│   │           ├── ActrBridge.kt      # Actr SDK Bridge (Cyberage)WebRTC Letter/AI Communications)
│   │           ├── TabModule.kt       # RN NativeModule: Tab Operation
│   │           └── UserInfoModule.kt  # RN NativeModule: User information
│   └── settings.gradle       # Gradle Project Configuration (inclusive) autolink + Manual include)
│
├── actrium-runtime -> ../../../packages/actrium
│                              # React Native Business level. Source code in place. monorepo packages
│
├── index.js                  # RN Entry, registration. actrium-runtime Root Component
├── metro.config.js           # Metro Packing Configuration (By) node_modules Go on, get heavy!
├── package.json              # Root Dependence (Land Dependence)RN The original bank hereby announces its support. autolink)
└── babel.config.js
```

## Share Dependencies

```
packages/
├── actrium-runtime/  # Share React Native Business Run-time
├── askit/          # Share TS Library
├── keel/           # Keel Plugin Sandbox Engine
```

## Key structural decision-making

- **Double node_modules**: Android Root Directory `node_modules` For Android autolink Use`packages/actrium/node_modules` Yes. RN Actual reliance at the operational level.Metro Pass. `extraNodeModules` + `blockList` - To the heavy. react/react-native.
- **New architecture (New Architecture)**: Enabled (`newArchEnabled=true`),use Hermes Engines,TurboModules, Fabric.
- **RN Original Library Link**: Most libraries go through root `package.json` + autolink automatic links;a few libraries (e. g. `react-native-safe-area-context`Yes `settings.gradle` Manually include.
- **Node Environment**: Gradle From Build node Path passed `android/local.properties` It's... `reactNative.nodeExecutableAndArgs` Configure, Solve nvm The problem of environmental invisibility.

## Build Note

- Ensure before building `android/local.properties` Medium `reactNative.nodeExecutableAndArgs` Point right. node Absolute Path
- Add RN Add to the original library at the same time Android Root `package.json`(autolinkand `packages/actrium/package.json`(JS Dependence)
- `android/build.gradle` Medium `REACT_NATIVE_NODE_MODULES_DIR` Provision to third-party banks react-native Path to avoid internal calls node Parsing
