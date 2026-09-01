# Actrium for Android

This directory contains the React Native entry point and Android native host for
the Actrium browser.

## Development

Install the shared packages from the repository root:

```sh
pnpm install
```

Prepare the runtime link and install the Android app dependencies:

```sh
cd android
yarn install
yarn actro-runtime
```

Set `sdk.dir` in `android/local.properties` to the local Android SDK. If Node is
not on Gradle's default path, also configure
`reactNative.nodeExecutableAndArgs` for the current machine.

Start Metro and the application in separate terminals:

```sh
yarn dev
yarn android
```

## Verification

Run the repository-level commands from the repository root:

```sh
npm run bundle:android
npm run verify:android
```

The `android/` subdirectory contains the Gradle project and platform lifecycle
code. The `actro-runtime` link points to `../packages/actrium`; `index.js` only
registers the shared runtime application.
