# Actrium Development Guidance (executed in this directory)

## 1) Generate RN Bundle(keel I'll have to repackage it after it's updated)
System bundle(includes StartPage/History/Bookmarks/Debug, also includes `RuntimeApp` Host:

`./scripts/bundle-actrium-runtime.sh --target system --dev`

## 2) Build CounterApp(Guest)
Follow the [CounterApp instructions in ACTR Hub](https://github.com/Actrium/actr-hub/tree/main/counterapp#build-the-packages).

## 3) Compile
`xcodebuild -workspace Actrium.xcworkspace -scheme Actrium build`

## 4) Run
Directly initiate:

`open "$(xcodebuild -workspace Actrium.xcworkspace -scheme Actrium -showBuildSettings 2>/dev/null | awk '/BUILT_PRODUCTS_DIR/ {print $3; exit}')/Actrium.app"`

## 5) View Log
IPC Debug log:

`tail -f /tmp/actrium_ipc_debug.log`

System Log (inclusive) RN/JS Error):

`log stream --predicate 'process CONTAINS "Actrium" OR process CONTAINS "ActriumRenderer"' --style compact`
