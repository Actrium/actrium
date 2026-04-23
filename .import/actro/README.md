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

#### Helper 脚本 (`/af`)

项目提供了 `af` 快捷脚本，整合了常用的构建与开发命令：

```shell
./Scripts/af <command>
```

| 命令 | 缩写 | 说明 |
|------|------|------|
| `install` | `i` | 安装依赖 (React Native + CocoaPods) |
| `bundle` | `b` | 打包 React Native Bundle |
| `build` | `bd` | 本地测试构建 (生成 IPA)，默认 Release |
| `build pkg` | `bd pkg` | 本地测试构建 (Debug 模式) |
| `check` | `c` | 检查项目配置 |
| `upload` | `u` | 构建并上传 TestFlight |
| `clean` | `cl` | 清理构建产物和缓存 |
| `help` | `h` | 显示帮助信息 |

示例：

```shell
./Scripts/af install           # 安装所有依赖
./Scripts/af bundle            # 打包 RN Bundle
./Scripts/af build             # 本地测试构建 (Release)
./Scripts/af build pkg         # 本地测试构建 (Debug)
./Scripts/af check             # 检查配置
./Scripts/af upload            # 上传到 TestFlight
./Scripts/af clean             # 清理构建产物
```

#### 说明

- `Subsystem` 里的 `react-native-webview` 依赖 `patch-package` 自动打补丁，安装依赖后会在 `postinstall` 执行。
- Metro 需要固定解析到 `Subsystem/node_modules/react-native-webview`，不能落到 `supermark/node_modules/react-native-webview`，否则 iOS 下可能出现旧架构 `shouldStartLoadWithLockIdentifier` 相关崩溃。
