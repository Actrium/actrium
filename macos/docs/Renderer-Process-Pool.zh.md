# Actrium Renderer Process Pool

## 1. 目标

将当前按构建脚本复制 `RCTViewService0...3.xpc` 的 ViewBridge 架构替换为
Chrome 风格的 Broker/Renderer 架构：

- 安装包中只包含一个 `ActriumRenderer.app` Helper；
- Broker 可以多次启动同一份 Helper 可执行文件；
- 每个 Helper 实例拥有独立地址空间、Hermes VM、React Native Host 和沙箱；
- 大型二进制及资源不随池容量复制；
- 保持现有 fingerprint、Tab、预热、尺寸更新、系统应用和崩溃隔离语义。

## 2. 业务不变量

迁移必须保持以下行为：

1. `system.*` fingerprint 使用系统 Renderer。
2. 相同 fingerprint 的多个 Tab 共用同一个 Renderer 进程和 RCTHost，但每个
   Tab 保持独立 RN root、属性和渲染表面。
3. 不同普通 fingerprint 使用不同 Renderer 进程，互不共享可变运行状态。
4. Tab 关闭时只销毁该 Tab 的 root；Renderer 内最后一个 Tab 关闭后进入空闲状态。
5. Renderer 崩溃不能终止 Actrium 或其他 Renderer；受影响 Tab 可重新加载。
6. Focus/Overview 切换不重建 Renderer，仍通过实时尺寸与属性更新完成。
7. StartPage、History、Bookmarks、ACTR 脚本及左右面板属性保持原有数据结构。
8. 鼠标、键盘、滚轮、缩放手势和窗口 resize 必须路由到正确 Tab。

## 3. 进程模型

```text
Actrium.app
└── RendererProcessPool（Broker）
    ├── system.*  ──> ActriumRenderer PID A
    │                 ├── Tab root 1 + CAContext 1
    │                 └── Tab root 2 + CAContext 2
    ├── fingerprint X -> ActriumRenderer PID B
    │                 └── Tab root 3 + CAContext 3
    └── fingerprint Y -> ActriumRenderer PID C
                      └── Tab root 4 + CAContext 4
```

Renderer 身份是 Broker 生成的 UUID，不再是 XPC Bundle ID。Broker 保存：

- fingerprint -> rendererId
- tabId -> rendererId
- rendererId -> Process、IPC connection、PID、状态、引用计数

池的最大容量是运行时策略，不影响安装包体积。初始策略继续保留系统 Renderer 和
一个普通 Renderer 的预热能力。当前上限为：

```text
min(12, max(4, ceil(activeProcessorCount / 2)))
```

例如 10 核机器上限为 5。上限只是防止失控的安全阀，不会预先创建这些进程；
实际数量按 fingerprint 需求动态增长，收到内存压力时优先回收无 Tab 的普通
Renderer。

## 4. 启动与 IPC

Broker 为每个 Renderer 创建一对匿名 Unix `socketpair`。Broker 保留一端，另一端
作为 Helper 的标准输入文件描述符继承过去，然后使用 `Process` 多次启动同一个签名
Helper：

```text
ActriumRenderer
  --renderer-id <UUID>
  --fingerprint <fingerprint>
  --ipc-stdin
  --host-pid <PID>
```

通信使用已有 `ActriumIPC` 的长度前缀 Protobuf 帧。一个 Renderer 连接复用所有
Tab 消息，因此所有 Tab 级消息必须携带 `tab_id`。socket 启用 `SO_NOSIGPIPE`，
写入串行化并处理 partial write / `EINTR`，关闭对端只返回错误，不会用
`SIGPIPE` 杀死进程。

握手：

1. Broker 创建 `socketpair`。
2. Broker 把子进程端作为 stdin 启动 Helper。
3. Helper 接管已连接的描述符并发送 `renderer_ready`。
4. Broker 将 Renderer 标记为 ready。
5. 第一个 Tab 配置触发 RN root 和 CAContext 创建。
6. Helper 返回该 Tab 的 `context_id`。

配置、尺寸、输入、标题、导航、日志、崩溃和系统应用动作都走类型化 Protobuf。
系统动作使用 `HostEvent` 回到 Broker，Broker 只接受固定白名单，再转换成本地
Notification；Renderer 不依赖跨沙箱的 `DistributedNotificationCenter`。
这样也去掉了原有 pendingTabId/internalId/FIFO 绑定竞态。

## 5. 渲染与输入

每个 Renderer Tab 创建：

- `ResizableContainerView`
- `RCTSurfaceHostingProxyRootView`
- 隐藏的 `NSWindow`（事件路由与 First Responder）
- 私有 `CAContext`

Helper 将 Tab 根 layer 设置为 `CAContext.layer`，把 `contextId` 通过 IPC 发给
Broker。主进程使用 `CALayerHost` 显示远程 layer。该路径只共享 WindowServer /
CoreAnimation 管理的表面，不复制每帧像素。

主进程的 `RendererSurfaceHostView` 捕获 AppKit 事件并通过 IPC 发送标准化事件。
Helper 根据 Tab 对应的隐藏窗口重建 `NSEvent`，由窗口发送给 RN 视图。Overview
模式禁止命中测试；Focus 模式获取 First Responder 并接收键盘事件。捏合返回
Overview 在主进程处理，不依赖远程 NSView。

跨进程 Accessibility 走同一条 IPC，不依赖成熟第三方库（Chrome Remote Cocoa
不开放）。Helper 把 NSView / `NSAccessibility` 树快照成 `AccessibilityTree`，
主进程用标准 `NSAccessibilityElement` 代理这棵树。VoiceOver 看到的是系统角色
（按钮、分组、文本），按压等动作再经 IPC 回到 Renderer。像素层共享仍然不是
完整 NSView 语义；这是最小可用 AX 代理，不是完整远程 AX 树同步。

## 6. 生命周期与故障恢复

Renderer 状态：

```text
idle -> launching -> ready -> busy
                    |         |
                    +-> crashed/terminated
```

- `allocate(tab,fingerprint)` 复用同 fingerprint Renderer，否则启动新实例。
- Tab 在不重建宿主 NSView 的情况下改变 fingerprint 时，装载前必须重新核对身份：
  先在旧 Renderer 销毁该 Tab，再绑定新 Renderer，并强制替换 `CALayerHost`
  context；不能因为 context 数值恰好相同而保留旧进程的图层。
- `destroyTab(tab)` 释放 root、CAContext、隐藏窗口和路由表。
- 最后一个普通 Tab 关闭后，Renderer 可保留短暂 warm TTL，再优雅退出。
- Helper 退出时，Broker 先清理全部映射，再通知受影响 Tab；宿主最多自动重试三次，
  使用原始配置在替代 Renderer 上恢复。
- critical memory pressure 下只回收没有 Tab 的 warm Renderer，不迁移活跃 Tab。
- Actrium 退出时向每个 Helper 发送 terminate 并结束子进程。

## 7. 打包

目标结构：

```text
Actrium.app
├── Contents/MacOS/Actrium
├── Contents/Helpers/ActriumRenderer.app
│   ├── Contents/MacOS/ActriumRenderer
│   ├── Contents/Frameworks/hermes.framework
│   └── Contents/Resources/system.jsbundle
└── Contents/Resources/...
```

React Native、Hermes、Renderer 业务实现和 `system.jsbundle` 只存在于一个 Helper
Bundle 中；主程序不再链接 RN/Hermes。运行 N 个实例时，磁盘仍只有这一份可执行文件
和资源，内核也能共享同一 Mach-O 的只读代码页；每个实例的堆、Hermes VM、RCTHost
和沙箱则保持独立。这就是这里所说的 Chrome 式“单一程序，多进程实例”，不需要为
每个池槽复制 Bundle。

已删除：

- `Generate XPC Service Pool` build phase 与 `generate_xpc_pool.sh`；
- `XPCServicePool` 与未使用的 `ViewBridgeMultiProcessTabView`；
- `RCTViewService0...3.xpc` 槽位逻辑。

## 8. 验证门槛

完成迁移需要同时满足：

1. 干净 Debug 构建和相关 ActriumTests 通过。
2. App 内没有 `RCTViewService*.xpc`，只有一个 `ActriumRenderer.app`。
3. 打开 system app 与普通 fingerprint 时观察到不同 Renderer PID。
4. 同 fingerprint 两个 Tab 映射到同一 PID、不同 contextId。
5. Focus/Overview、resize、鼠标、键盘、滚轮、面板切换可用。
6. 人工终止一个 Renderer 后主进程和其他 Renderer 保持运行，受影响 Tab 自动恢复。
7. `codesign --verify --deep --strict` 通过。
8. 记录迁移前后 App 总体积及主要构成。

## 9. 本次验证结果

- 全新 DerivedData 的无签名 Debug 构建成功。
- `ActriumIPC` 传输测试 5/5 通过，覆盖多路 Tab、HostEvent 和关闭对端不触发
  `SIGPIPE`。
- URL 路由与进程池策略测试通过；策略包含硬件核数边界、`system.*` 归一化、
  普通 fingerprint 精确共享和跨身份禁止共享。
- History 与 Bookmarks 实测同一 Renderer PID、不同 `CAContext`。
- 人工终止 system Renderer 后，两个 Tab 均在替代进程恢复，主进程及普通预热
  Renderer 不受影响。
- 签名产物 `codesign --verify --deep --strict` 通过；Helper 沙箱、App Group、
  JIT 和网络客户端 entitlement 生效。
- 旧 Debug App 约 347 MiB；新 Debug App 为 143,144 KiB（约 139.8 MiB），减少约
  60%。安装内容为
  0 个 `.xpc`、1 个 `ActriumRenderer.app`、1 个 `system.jsbundle`。
- ACTR / Keel Guest 本轮验收范围只到宿主视图结构、装载链路和 fingerprint 隔离边界；
  其应用内部行为留给后续格式调整后再做完整适配。
- 薄 Helper 当前不链接移动端页面才使用的 WebView、DeviceInfo、Reanimated 和
  GestureHandler 原生实现；React Native 生成模块表仍会对未链接的可选 WebView
  provider 记录一次探测告警，但当前 macOS 可达的 History / Bookmarks 不依赖它，
  两个页面的 Surface 均实测进入 Running 状态。后续 ACTR / Keel Guest 定型时再按实际
  capability 选择原生模块或 macOS 平台视图，避免提前把整套移动依赖塞回安装包。

## 10. 签名与平台边界

本机开发证书可对 Helper 与主程序做嵌套签名并运行验证。Xcode 自动签名仍需要为新
Bundle ID `io.actrium.renderer` 创建 provisioning profile；这是账号侧准备项，
不影响手动开发签名或架构本身。

`CAContext`、`CALayerHost` 与当前 ViewBridge 一样属于 macOS 私有框架能力，不适合
Mac App Store。Actrium 当前已使用私有 ViewBridge，因此本次迁移不新增发布渠道类别，
但必须在支持的 macOS 版本上做运行探测和失败降级。

Chrome Remote Cocoa 的完整 AX 代理没有可引用的成熟开源库。Actrium 用最小实现：
Renderer 导出 `AccessibilityTree`，`RendererSurfaceHostView` 作为 AX 分组暴露
子节点，动作经 `accessibility_action` 回传。键盘焦点仍走输入 IPC。没有远程
完整 AX API 覆盖（自定义参数、滚动区域细节、实时文本选择范围）。
