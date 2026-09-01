# Actrium for iOS

This directory contains the iOS native host for the Actrium browser.

## Development

Install the monorepo dependencies from the repository root, then prepare the
runtime link and CocoaPods workspace:

```sh
pnpm install
cd ios
./Scripts/ensure-actro-runtime.sh
pod install
```

Start Metro from the shared runtime package:

```sh
cd ../packages/actrium
yarn dev
```

Open `ios/Actro.xcworkspace` in Xcode, or run the repository verification
command:

```sh
npm run verify:ios
```

Use `./Scripts/af <command>` for the existing install, bundle, build, upload,
clean, and configuration-check helpers. Run `./Scripts/af help` for the complete
command list.

`ActroRuntime` is a compatibility symlink to `../packages/actrium`; the iOS host
does not maintain a separate React Native runtime copy.
