### askaway-ipad

#### 开发

0. 拉取需要 `git submodule update --init --recursive`
1. 拉取 askit，rill 库
2. 执行命令

```shell
cd rill && bun install
cd askit && bun install
```

3. 修改 `.xcode.env.local` 中 node 地址
4. 执行命令

```shell
ln -s ../loom Subsystem
cd .. && pod install
```

5. 运行

```shell
cd Subsystem && yarn dev
```

#### 说明

- `Subsystem` 里的 `react-native-webview` 依赖 `patch-package` 自动打补丁，安装依赖后会在 `postinstall` 执行。
- Metro 需要固定解析到 `Subsystem/node_modules/react-native-webview`，不能落到 `supermark/node_modules/react-native-webview`，否则 iOS 下可能出现旧架构 `shouldStartLoadWithLockIdentifier` 相关崩溃。
