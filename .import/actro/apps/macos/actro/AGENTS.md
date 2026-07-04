# Actro 开发指引（在本目录执行）

## 1) 生成 RN Bundle（rill 更新后必须重打包）
System bundle（包含 StartPage/History/Bookmarks/Debug，同时也包含 `ActrApp` 宿主）：

`./scripts/bundle-actro-runtime.sh --target system --dev`

## 2) 构建 CounterApp（Guest）
`cd ../../../packages/actro-runtime/actr-app-examples/counterapp && bun run build`

## 3) 编译
`xcodebuild -workspace Actro.xcworkspace -scheme Actro build`

## 4) 运行
直接启动：

`open "$(xcodebuild -workspace Actro.xcworkspace -scheme Actro -showBuildSettings 2>/dev/null | awk '/BUILT_PRODUCTS_DIR/ {print $3; exit}')/Actro.app"`

启动后自动打开 CounterApp（推荐用于验证 rill/Bundle 是否匹配）：

`open -a "$(xcodebuild -workspace Actro.xcworkspace -scheme Actro -showBuildSettings 2>/dev/null | awk '/BUILT_PRODUCTS_DIR/ {print $3; exit}')/Actro.app" --args --url "actro://$(cd ../../../packages/actro-runtime/actr-app-examples/counterapp && pwd)"`

## 5) 查看日志
IPC 调试日志：

`tail -f /tmp/actro_ipc_debug.log`

系统日志（含 RN/JS Error）：

`log stream --predicate 'process CONTAINS "Actro" OR process CONTAINS "RCTViewService"' --style compact`
