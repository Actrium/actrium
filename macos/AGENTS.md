# Actro Development Guidance (executed in this directory)

## 1) Generate RN Bundle(keel I'll have to repackage it after it's updated)
System bundle(includes StartPage/History/Bookmarks/Debug, also includes `RuntimeApp` Host:

`./scripts/bundle-actro-runtime.sh --target system --dev`

## 2) Build CounterApp(Guest)
`cd ../../../packages/actrium/keel-guest-examples/counterapp && bun run build`

## 3) Compile
`xcodebuild -workspace Actro.xcworkspace -scheme Actro build`

## 4) Run
Directly initiate:

`open "$(xcodebuild -workspace Actro.xcworkspace -scheme Actro -showBuildSettings 2>/dev/null | awk '/BUILT_PRODUCTS_DIR/ {print $3; exit}')/Actro.app"`

Auto Open After Start CounterApp(Recommended for validation) keel/Bundle Matches:

`open -a "$(xcodebuild -workspace Actro.xcworkspace -scheme Actro -showBuildSettings 2>/dev/null | awk '/BUILT_PRODUCTS_DIR/ {print $3; exit}')/Actro.app" --args --url "actro://$(cd ../../../packages/actrium/keel-guest-examples/counterapp && pwd)"`

## 5) View Log
IPC Debug log:

`tail -f /tmp/actro_ipc_debug.log`

System Log (inclusive) RN/JS Error):

`log stream --predicate 'process CONTAINS "Actro" OR process CONTAINS "ActroRenderer"' --style compact`
