# Actro Android

## 开发

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

2. 安装 Android app 依赖并准备 runtime 软链

```shell
cd ../../apps/android/actro
yarn install
yarn actro-runtime
```

3. 检查 `android/local.properties`

确保 `sdk.dir` 指向本机 Android SDK，并设置当前机器可用的
`reactNative.nodeExecutableAndArgs`。

4. 启动 Metro 和 Android app

```shell
yarn dev
yarn android
```

## 验证

从仓库根目录优先运行：

```shell
npm run bundle:android
npm run verify:android
```

需要在 Android app 目录下调试时，可使用等价的底层命令：

```shell
npx react-native bundle --platform android --dev false --entry-file index.js --bundle-output /tmp/actro-android-index.bundle --assets-dest /tmp/actro-android-assets --reset-cache

cd android
./gradlew :app:assembleDebug --console=plain --stacktrace --no-daemon
```

## 结构

- `actro-runtime` 是指向 `../../../packages/actro-runtime` 的软链。
- `index.js` 只负责注册共享 runtime 里的 `ActrApp`。
- `android/` 保留 Android 原生工程、Gradle 配置和平台生命周期代码。
- `packages/actr-kotlin` 承载迁出的 Android Actr Kotlin 包。
