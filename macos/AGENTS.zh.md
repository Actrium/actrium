# Actrium 开发指引（在本目录执行）

## 1) 生成 RN Bundle（keel 更新后必须重打包）
System bundle（包含 StartPage/History/Bookmarks/Debug，同时也包含 `RuntimeApp` 宿主）：

`./scripts/bundle-actrium-runtime.sh --target system --dev`

## 2) 构建 CounterApp（Guest）
参阅 [ACTR Hub 中的 CounterApp 说明](https://github.com/Actrium/actr-hub/tree/main/counterapp#build-the-packages).

## 3) 编译
`xcodebuild -workspace Actrium.xcworkspace -scheme Actrium build`

## 4) 运行
直接启动：

`open "$(xcodebuild -workspace Actrium.xcworkspace -scheme Actrium -showBuildSettings 2>/dev/null | awk '/BUILT_PRODUCTS_DIR/ {print $3; exit}')/Actrium.app"`

## 5) 查看日志
IPC 调试日志：

`tail -f /tmp/actrium_ipc_debug.log`

系统日志（含 RN/JS Error）：

`log stream --predicate 'process CONTAINS "Actrium" OR process CONTAINS "ActriumRenderer"' --style compact`
