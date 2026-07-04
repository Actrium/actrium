### Actro iOS

#### 开发

0. 从仓库根目录准备 Supramark React Native 原生库

```shell
npm run prepare:supramark-native
```

这些原生产物体积较大，不直接提交到源码包里；fresh checkout 后需要本地生成一次。

1. 安装共享包依赖

```shell
cd packages/rill && bun install
cd ../askit && bun install
cd ../actro-runtime && bun install
```

2. 准备 `ActroRuntime` symlink 并安装 CocoaPods

```shell
cd ../../apps/ios/actro
./Scripts/ensure-actro-runtime.sh
pod install
```

3. 运行 Metro

```shell
cd ActroRuntime && yarn dev
```

4. 使用 Xcode 打开 `Actro.xcworkspace`，或用命令行验证构建

```shell
xcodebuild -workspace Actro.xcworkspace -scheme Actro -configuration Debug -destination 'generic/platform=iOS' CODE_SIGNING_ALLOWED=NO build
```

#### 验证

从仓库根目录优先运行：

```shell
npm run bundle:ios
npm run verify:ios
```

需要在 iOS app 目录下调试时，可使用等价的底层命令：

```shell
(cd ActroRuntime && node node_modules/react-native/scripts/bundle.js --platform ios --dev false --entry-file index.ts --bundle-output /tmp/actro-ios-index.bundle --assets-dest /tmp/actro-ios-assets --reset-cache)
xcodebuild -workspace Actro.xcworkspace -scheme Actro -configuration Debug -destination 'generic/platform=iOS' CODE_SIGNING_ALLOWED=NO build
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

- `ActroRuntime` 里的 `react-native-webview` 依赖 `patch-package` 自动打补丁，安装依赖后会在 `postinstall` 执行。
- Metro 需要固定解析到 `ActroRuntime/node_modules/react-native-webview`，不能落到 `supramark/node_modules/react-native-webview`，否则 iOS 下可能出现旧架构 `shouldStartLoadWithLockIdentifier` 相关崩溃。
- `ActroRuntime` 是指向 `../../../packages/actro-runtime` 的软链，iOS app 不再维护自己的 React Native runtime 副本。
