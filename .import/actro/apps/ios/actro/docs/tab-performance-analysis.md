# AskAway iPhone 标签页性能问题分析与解决方案

> 本文档主要从 **iPhone 端** 视角进行分析。iPad 端使用 Focus/Overview 模式切换，iPhone 端使用 `.sheet` 全屏展开 + TabView 底部导航，交互路径不同，但底层内存问题一致。

## 1. 问题现象

| 问题 | iPhone 端表现 |
|------|--------------|
| 新建标签变慢 | 标签越多，新建标签响应越慢，严重时 App 崩溃（OOM） |
| 设备发烫 | 开多个标签后，手机明显发热，电量骤降 |
| 切回旧标签卡顿 | 已打开过的标签切回时仍需重新创建 RootView，体验卡顿 |

---

## 2. iPhone 端标签交互流程

```
PhoneMainView (TabView 底部导航)
  ├─ 书签页 (PhoneSystemAppView → SingleTabRootView)
  ├─ 标签列表页 (PhoneTabListView → LazyVStack)
  ├─ 设置页 (PhoneSystemAppView → SingleTabRootView)
  └─ 搜索页 (PhoneSystemAppView → SingleTabRootView)

用户从标签列表页选择一个标签 →
  .sheet(item: $fullscreenTab) 弹出全屏 →
    PhoneBrowserView(tab) →
      SingleTabRootView(tab) →
        CanvasReactNativeHost.shared.viewForTab(tabId, props) →
          viewCache[tabId] 缓存 RCTRootView

用户退出全屏（RN 侧通知或手势） →
  takeSnapshot() 截图保存 →
  fullscreenTab = nil →
  sheet 收起 →
  回到标签列表页（此时 viewCache 仍持有 RootView）
```

**关键问题**：退出全屏回到列表后，该标签的 `RCTRootView` **并未释放**，仍然常驻 `viewCache`。下次再打开同一标签，会从缓存取回（快速），但也意味着每个曾经打开过的标签都永远占有一份完整的 React 运行时内存。

---

## 3. 根因分析

### 3.1 每个 Tab 的 RCTRootView 常驻内存（核心问题）

**现状**：`CanvasReactNativeHost.viewCache` 是强引用字典，每个曾打开过的 Tab 的 `RCTRootView` 永远不会被释放。

```
CanvasReactNativeHost.swift:88
private var viewCache: [UUID: RCTRootView] = [:]

// 只有 closeTab 才清理
BrowserViewModel.swift:437-438
func closeTab(id: UUID) {
    CanvasReactNativeHost.shared.removeViewForTab(tabId: id)
    ...
}
```

**问题**：
- 每个 `RCTRootView` 内含完整的 React 组件树 + JS 桥接 + Yoga 布局引擎
- iPhone 内存比 iPad 更有限（典型设备 4-6GB），累积更快触及上限
- N 个曾打开标签 = N 套 React 运行时 → 内存线性增长
- 即使标签不在全屏状态，后台 RootView 的 React 渲染循环仍在运行

**对比浏览器**：Chrome/Safari 只为当前可见标签维持活跃渲染引擎，后台标签进程被挂起或释放。

---

### 3.2 后台 RootView 中的 React 组件持续消耗 CPU

**现状**：每个 `RCTRootView` 即使不在屏幕上显示，其内部的 React 组件仍在执行：
- ActrBridge 的 `StreamPrintCallback` 持续接收流式消息，触发 `sendEvent` 更新组件
- 组件中的 Timer、Animation、WebSocket 等持续运行
- 每次事件触发 React re-render → Yoga 布局计算 → 视图属性更新

**问题**：
- N 个后台标签的 React 渲染循环叠加 → CPU 持续高负载 → 发烫
- 每个 `sendEvent` 都走 JS Bridge → 主线程队列竞争 → UI 卡顿

---

### 3.3 标签列表页的视图重建开销

**现状**：`PhoneTabListView` 中每个标签通过 `@ObservedObject var viewModel` 观察整个 `BrowserViewModel`。

```
PhoneTabListView.swift:39
ForEach(viewModel.tabs) { tab in
    PhoneTabCardView(tab: tab, viewModel: viewModel, ...)
}
```

**问题**：
- `tabs` 是 `@Published` 值类型数组，任何 Tab 属性变更 → 整个 ViewModel `objectWillChange` → 所有卡片重绘
- 例如一个标签收到未读消息 → 更新 `hasUnreadMessage` → 所有卡片重新计算 body
- 标签越多，单次变更的重绘成本越高

---

### 3.4 tabs didSet 触发频繁磁盘保存

**现状**：

```
BrowserViewModel.swift:23-26
@Published var tabs: [Tab] = [] {
    didSet {
        saveTabs()  // ← 每次修改都触发持久化
    }
}
```

**问题**：
- `tabs` 是值类型数组，任何元素修改（标题、图标、红点等）都触发 `didSet`
- `saveTabs()` 编码所有 Tab 为 JSON 写 UserDefaults + 遍历所有 Tab 保存截图到磁盘
- `setTabUnreadStatus` 是高频操作 → 每次都触发完整的磁盘 I/O
- 在 iPhone 端，频繁磁盘写入加剧了发烫（闪存写入功耗高）

---

### 3.5 Tab 快照内存双重占用

**现状**：

```
BrowserViewModel.swift:58
@Published var tabSnapshots: [UUID: UIImage] = [:]
```

`PhoneMainView.takeSnapshot` 截取全屏画面保存到 `tabSnapshots`，同时 `setTabSnapshot` 调用 `saveSnapshot` 写入磁盘。

**问题**：
- 全屏 UIImage 约 3-6 MB（iPhone 14 Pro 分辨率），10 个标签 ≈ 30-60 MB
- 截图同时存在于内存（`tabSnapshots`）和磁盘（images 目录），双重占用
- 标签列表页的卡片预览只需要小缩略图，却持有全屏原图

---

### 3.6 缺乏标签数量上限

**现状**：代码中没有标签数量上限机制。

**问题**：
- 用户可以无限创建标签，没有任何兜底保护
- iPhone 内存更紧张，比 iPad 更容易触发 OOM
- Chrome 移动端约限制 50-100 个标签，Safari 也有类似限制

---

### 3.7 SingleTabRootView props 中的冗余传递

**现状**：

```swift
// SingleTabRootView.swift:248-260
var props: [String: Any] = [
    "singleTabMode": true,
    "tabs": [tabDict],       // ← 包装成数组，兼容旧逻辑
    "tab": tabDict,          // ← 又传了一次
    "currentTabId": tab.id.uuidString,
    ...
]
```

**问题**：
- 同一个 Tab 数据传递了两次（`tabs` 数组 + `tab` 对象）
- 脚本内容（`unifiedScript` 等）作为内联字符串传递，每次 props 更新都重新序列化

---

## 4. 浏览器标签管理策略参考

| 策略 | Chrome iOS | Safari iOS | Firefox iOS |
|------|-----------|-----------|-------------|
| **进程模型** | 后台标签 WebKit 进程挂起 | 后台标签完全冻结 | 后台标签释放渲染资源 |
| **内存回收** | 内存压力时自动丢弃后台标签渲染树 | 系统级内存压力回收 | 后台标签释放非关键资源 |
| **标签上限** | ~100 个，超出自动丢弃旧标签 | 无硬限制，但内存压力回收 | 无硬限制 |
| **恢复机制** | 切回时显示快照 + "重新加载" | 切回时重新加载 | 切回时重新加载 |
| **快照缓存** | 切走时截屏缩略图，切回时先显示截屏 | 类似 Chrome | 类似 Chrome |
| **懒加载** | 后台标签不执行 JS | 后台标签完全冻结 | 延迟执行后台标签 |

**核心原则**：**只保留当前活跃标签的完整渲染资源，后台标签仅保留恢复所需的最小元数据 + 快照缩略图**。

---

## 5. 解决方案

### 5.1 viewCache 改为 LRU 有限缓存（最高优先级）

> **为什么不能完全释放 RootView？** 释放后再次打开同一标签需要重新走 `createRootView`，包括 React 组件树构建 + JS 执行 + Yoga 布局 + 首次渲染，耗时约 500-1500ms，用户会看到 Loading 或空白闪烁。完全释放会导致每次重新打开都有加载等待，体验不如当前方案。

**方案**：将 `viewCache` 从无限缓存改为 LRU（最近最少使用）有限缓存，只保留最近打开的 2-3 个标签的 RootView。

**为什么 2-3 个够用**：iPhone 同一时间只有 1 个标签处于全屏状态。用户最常见的切换模式是"退出 A → 打开 B → 切回 A"，2-3 个缓存覆盖了这种来回切换场景。不常用的旧标签自动释放，避免了"10 个标签全驻留内存"的问题。

**改造 `CanvasReactNativeHost`**：

```swift
// 当前：无限缓存
private var viewCache: [UUID: RCTRootView] = [:]

// 改造后：LRU 有限缓存
static let maxCacheSize = 3  // iPhone 端保留 2-3 个

private var viewCache: [UUID: RCTRootView] = [:]
private var accessOrder: [UUID] = []  // 访问顺序队列（最近访问在尾部）

func viewForTab(tabId: UUID, initialProperties: [String: Any]) -> UIView {
    if let cachedView = viewCache[tabId] {
        // 命中缓存：移到队列尾部（标记为最近使用）
        accessOrder.removeAll { $0 == tabId }
        accessOrder.append(tabId)
        return cachedView
    }

    // 未命中：创建新 RootView
    let rootView = createRootView(initialProperties: initialProperties)
    if let rctRootView = rootView as? RCTRootView {
        viewCache[tabId] = rctRootView
        accessOrder.append(tabId)

        // 超过上限：释放最久未使用的 RootView
        while viewCache.count > maxCacheSize {
            let oldestTabId = accessOrder.removeFirst()
            viewCache.removeValue(forKey: oldestTabId)
        }
    }
    return rootView
}
```

**预期效果**：

| 场景 | 当前 | LRU 改造后 |
|------|------|-----------|
| 最近 2-3 个标签来回切换 | 即时打开 | 即时打开（缓存命中） |
| 第 4 个标签及更旧的 | 即时打开（全部缓存） | 需要重新创建（约 500-1500ms） |
| 内存占用 | N × ~40MB | max(3, N) × ~40MB |
| 10 个标签内存 | ~400MB | ~120MB |

---

### 5.2 快照占位：缓解 LRU 淘汰标签的加载体验

LRU 缓存未命中时需要创建新 RootView（约 500-1500ms）。用快照缩略图做过渡占位，让用户感知到的是"快照 → 真实内容"而非"空白 → 内容"。

**改造 `PhoneBrowserView`**：

```swift
struct PhoneBrowserView: View {
    let tab: Tab
    @ObservedObject var viewModel: BrowserViewModel
    var historyVM: HistoryViewModel
    @State private var isRootViewReady = false

    var body: some View {
        GeometryReader { geometry in
            ZStack {
                // 底层：快照占位（RootView 创建期间显示）
                if let snapshot = viewModel.tabSnapshots[tab.id] {
                    Image(uiImage: snapshot)
                        .resizable()
                        .aspectRatio(contentMode: .fill)
                        .frame(width: geometry.size.width, height: geometry.size.height)
                        .opacity(isRootViewReady ? 0 : 1)
                        .animation(.easeOut(duration: 0.3), value: isRootViewReady)
                }

                // 上层：真实 RN 内容
                SingleTabRootView(tab: tab, viewModel: viewModel, ...)
                    .id(tab.id)
                    .opacity(isRootViewReady ? 1 : 0)
                    .animation(.easeIn(duration: 0.2), value: isRootViewReady)
                    .onAppear {
                        // RootView 渲染就绪后，切换到真实内容
                        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                            isRootViewReady = true
                        }
                    }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(Color.black)
        }
    }
}
```

---

### 5.3 快照降采样（配合 5.2）

**方案**：退出全屏前截取缩略图而非全屏原图，大幅减少内存。

**当前已有快照机制**（`PhoneMainView.takeSnapshot`），需要增强：

1. **降采样**：快照缩放至约 200x400px（卡片预览尺寸），单张约 50KB，而非全屏 3-6MB
2. **时机**：在 sheet 收起前自动截取，而不是依赖额外逻辑
3. **按需加载**：标签列表页 `LazyVStack` 只加载可见区域快照，磁盘副本在滚动时按需读取

```swift
// 降采样快照
func downsampledSnapshot(_ image: UIImage, targetSize: CGSize) -> UIImage {
    let renderer = UIGraphicsImageRenderer(size: targetSize)
    return renderer.image { _ in
        image.draw(in: CGRect(origin: .zero, size: targetSize))
    }
}
```

---

### 5.4 退出全屏时的具体改造

**当前代码**：退出全屏时只截取快照，不释放 RootView。

```swift
// PhoneMainView.swift:101-109
.onReceive(NotificationCenter.default.publisher(for: .exitFullscreenFromRN)) { _ in
    if let tab = viewModel.tabs.first(where: { $0.id == fullscreenTab?.id }), !tab.title.isEmpty {
        takeSnapshot(for: tab) {
            fullscreenTab = nil  // ← sheet 收起，RootView 仍在 viewCache 中
        }
    } else {
        fullscreenTab = nil
    }
}
```

**改造后**：LRU 管理已由 `CanvasReactNativeHost` 自动处理，无需手动释放。退出全屏时只需截取快照缩略图：

```swift
.onReceive(NotificationCenter.default.publisher(for: .exitFullscreenFromRN)) { _ in
    if let tab = viewModel.tabs.first(where: { $0.id == fullscreenTab?.id }), !tab.title.isEmpty {
        takeThumbnailSnapshot(for: tab) {  // ← 改为降采样缩略图
            fullscreenTab = nil
        }
    } else {
        fullscreenTab = nil
    }
}
```

LRU 缓存的淘汰逻辑在下次打开新标签时自动触发——`viewForTab` 创建新 RootView 时，如果 `viewCache.count > maxCacheSize`，自动释放最久未访问的旧标签 RootView。

---

### 5.4 持久化防抖

**方案**：将 `saveTabs()` 从 `didSet` 移出，改为防抖保存。

```swift
private var saveTabsWorkItem: DispatchWorkItem?

func scheduleSaveTabs() {
    saveTabsWorkItem?.cancel()
    let workItem = DispatchWorkItem { [weak self] in
        self?.saveTabs()
    }
    saveTabsWorkItem = workItem
    DispatchQueue.main.asyncAfter(deadline: .now() + 2.0, execute: workItem)
}
```

截图保存移到 `setTabSnapshot` 中独立处理（已在当前代码中实现），`saveTabs()` 只负责 JSON 编码 + UserDefaults 写入，不再遍历截图。

---

### 5.5 设置标签数量上限

**方案**：参照 Chrome iOS，设置标签上限。

```swift
static let maxTabCount = 30  // iPhone 端建议更保守

func addNewTab(...) -> Tab? {
    guard tabs.count < Self.maxTabCount else {
        logger.warning("已达到标签上限 \(Self.maxTabCount)")
        // 可选：自动将最久未使用的标签标记为 suspended
        return nil
    }
    // ...
}
```

同时可实现 **LRU 淘汰**：超过上限时将最久未访问的 Tab 标记为 `suspended`（而非直接关闭），用户切回时可恢复。

---

### 5.6 细粒度视图更新

**方案**：避免一个 Tab 属性变更引发所有卡片重绘。

1. **Tab 改为 class**：`class Tab: ObservableObject, Identifiable`，每个属性用 `@Published`
2. **PhoneTabCardView 直接观察 Tab 对象**：`@ObservedObject var tab: Tab` 替代 `let tab: Tab` 值传递
3. **ViewModel 不再全局广播 Tab 属性变更**：只有结构性变更（增删 Tab）才触发 `objectWillChange`

---

### 5.7 内存压力监听与自动回收

**方案**：监听系统内存警告，自动降级后台标签。

```swift
// AppDelegate
func applicationDidReceiveMemoryWarning(_ application: UIApplication) {
    Task { @MainActor in
        BrowserViewModel.active?.handleMemoryWarning()
    }
}

// BrowserViewModel
func handleMemoryWarning() {
    // 1. 释放所有非活跃 Tab 的 RootView
    for tab in tabs where tab.id != currentTabId {
        CanvasReactNativeHost.shared.removeViewForTab(tabId: tab.id)
    }
    // 2. 释放快照内存副本（磁盘副本保留，按需加载）
    tabSnapshots.removeAll()
}
```

---

### 5.8 SingleTabRootView props 冗余清理

**方案**：移除 `makeProperties` 中的 `tabs` 数组包装，只传 `tab` 单对象。

```swift
var props: [String: Any] = [
    "singleTabMode": true,
    "tab": tabDict,              // ← 只传一次
    "currentTabId": tab.id.uuidString,
    // ... 移除 "tabs": [tabDict]
]
```

---

## 6. 关键问题：释放后台 RootView 后，新消息通知怎么办？

当前 `viewCache` 保持所有 Tab 的 `RCTRootView` 常驻，目的是让后台标签的 React 组件持续运行，从而接收实时消息并更新 UI。释放 RootView 后需要确保通知机制不受影响。

### 6.1 当前消息通知链路分析

**链路 A：Actr 实时消息（AI 对话流式响应）**

```
ActrBridge.actorInstances[tabId] → StreamPrintCallback.onStream()
  → bridge.sendStreamEventToRN("ActrStreamMessage")
  → 当前全屏标签的 RN 组件接收并渲染
```

关键点：**ActrBridge 是独立的 Native Module，不依赖 RCTRootView**。`actorInstances` 按 `tabId` 管理连接，通过 `RCTEventEmitter` 广播事件。即使某个 Tab 的 RootView 被释放，只要 Bridge 存在，事件仍会发送——只是没有组件在监听。

**链路 B：Tab 未读红点通知**

```
RN 侧调用 TabModule.setTabUnreadStatus(tabId, hasUnread)
  → 更新 viewModel.tabs[index].hasUnreadMessage
  → SwiftUI 观察到变更，更新标签列表卡片红点
```

关键点：**完全不依赖 RCTRootView 缓存**。通过 Native Module 直接修改 ViewModel，SwiftUI 自动响应。

### 6.2 iPhone 端场景分析

iPhone 端的交互模型比 iPad 端更简单——**同一时间最多只有 1 个标签处于全屏状态**。这意味着：

- 当用户在标签 A 全屏时，标签 B 的 RootView 不在视图树中
- 退出全屏后，标签 A 的 RootView 也不在视图树中
- **无论是否释放 viewCache，非全屏标签的 RootView 都不在视图树中，无法接收渲染事件**

所以在 iPhone 端，释放 `viewCache` 对消息通知的影响实际上更小：

| 功能 | 当前行为 | 释放 viewCache 后 |
|------|---------|-----------------|
| 红点通知 | 正常 | 正常（TabModule 直接改 ViewModel） |
| 全屏标签的流式消息 | 正常（有 RootView 接收） | 正常（全屏标签是唯一活跃 RootView） |
| 后台标签的流式消息 | sendEvent 发出但无组件渲染 | 同样无组件渲染（结果一样） |
| 切回标签恢复对话 | 从 viewCache 取回旧 RootView（快但有状态残留风险） | 创建新 RootView（需恢复策略） |

### 6.3 解决方案：分层释放 + 消息代理

核心思路：**释放渲染层（RCTRootView），但保留连接层（ActrBridge）和数据层（RN 侧状态）**。

#### 方案 A：Native 层消息代理（推荐）

在 Native 层为后台标签建立消息代理，即使 RootView 被释放，消息也不会丢失：

```
┌─────────────────────────────────────────────────┐
│                  Native Layer                    │
│                                                  │
│  ActrBridge (独立于 RootView，始终存活)           │
│    ├─ actorInstances[tabId] → 连接维持           │
│    ├─ 收到流式消息 → 判断是否全屏标签              │
│    │     ├─ 全屏：forwardToRN(msg) 正常推送       │
│    │     └─ 非全屏：存入消息队列 + 触发红点        │
│    └─ onTabResumed → flushQueuedMessages 批量推送 │
│                                                  │
│  TabMessageProxy (新增)                          │
│    ├─ perTabQueues: [UUID: [Message]]            │
│    ├─ onStreamMessage(tabId, msg) {              │
│    │     if isActiveTab(tabId) {                 │
│    │       forwardToRN(msg)                      │
│    │     } else {                                │
│    │       queue.append(msg)                     │
│    │       notifyUnread(tabId)                   │
│    │     }                                       │
│    └─ onTabResumed(tabId) {                      │
│          flushQueuedMessages(tabId)              │
│        }                                         │
└─────────────────────────────────────────────────┘
```

**实现要点**：

1. **ActrBridge 中增加消息代理逻辑**：
   - 当收到后台 Tab 的流式消息时，不再直接 `sendEvent`
   - 改为将消息缓存到 `perTabQueues[tabId]`
   - 同时调用 `TabModule.setTabUnreadStatus(tabId, true)` 触发红点

2. **Tab 恢复时重放消息**：
   - 创建新 RootView 后，将队列中缓存的消息通过 `sendEvent` 批量推送
   - RN 组件根据 `sessionId` 等字段去重，恢复对话上下文

3. **ActrBridge 生命周期与 RootView 解耦**：
   - `actorInstances` 不随 RootView 释放而清理
   - 仅在 `closeTab` 时调用 `shutdown(tabId:)`
   - 退出全屏时不调用 `shutdown`，保持连接活跃

#### 方案 B：RN 侧消息持久化

在 RN 侧利用状态管理（Redux/Context/Zustand），将对话历史持久化到全局 Store：

```
RN 侧：
  - 每个 Tab 的对话历史存入全局 Store（不随组件卸载而丢失）
  - RootView 重建时，从 Store 恢复对话上下文
  - 相当于"刷新页面但保留聊天记录"
```

优点：实现简单，无需 Native 层改动
缺点：流式消息在 RootView 不存在期间会丢失中间过程

#### 方案 C：混合方案（推荐落地路径）

结合 A 和 B，分阶段实施：

| 阶段 | 策略 | 效果 |
|------|------|------|
| **Phase 1** | 退出全屏时释放 RootView + 截取缩略图；红点正常；再次打开时重建 RootView + 从 RN Store 恢复对话历史 | 内存大幅降低，红点正常，对话历史可恢复 |
| **Phase 2** | 增加 Native 消息代理，缓存后台 Tab 的流式消息；Tab 恢复时批量重放 | 流式消息零丢失，体验与当前一致 |

### 6.4 需要确认的问题

1. **RN 侧对话状态存储在哪里？** 如果在组件内部 state，RootView 释放后会丢失。如果在全局 Store（Redux/Zustand），则可以恢复。
2. **ActrBridge 的 `actorInstances` 是否可以跨 RootView 生命周期存活？** 当前 `shutdown` 只在 RN 侧主动调用时触发，理论上可以。
3. **流式消息中间帧丢失是否可接受？** AI 对话场景下最终文本可通过 Store 恢复；实时协作场景下中间帧不可丢失，需要方案 A 的消息代理。

---

## 7. 优先级与实施路线

| 阶段 | 方案 | 预期收益 | 复杂度 |
|------|------|----------|--------|
| **P0 紧急** | 5.1 viewCache LRU 有限缓存 | 内存从 O(N)→O(3) | 低 |
| **P0 紧急** | 5.5 标签数量上限 | 防止无限增长 | 低 |
| **P0 紧急** | 5.7 内存压力监听 | 系统级兜底保护 | 低 |
| **P1 重要** | 5.2 快照占位过渡 | 缓解 LRU 未命中的加载感 | 中 |
| **P1 重要** | 5.3 快照降采样 | 快照内存降 90% | 低 |
| **P1 重要** | 5.4 持久化防抖 | 减少 I/O 阻塞 | 低 |
| **P2 改进** | 5.6 细粒度视图更新 | 减少列表页重绘 | 中 |
| **P2 改进** | 5.8 props 冗余清理 | 减少桥接开销 | 低 |

---

## 8. 预期效果

实施 P0 方案后：
- **内存占用**：从 N × ~40MB 降至 min(N, 3) × ~40MB + N × 50KB缩略图
- **最近标签切换**：仍从缓存即时打开，体验与当前一致
- **旧标签重新打开**：需要约 500-1500ms 创建 RootView，但快照缩略图做过渡，用户感知为"快照淡入 → 真实内容"
- **发烫问题**：大幅缓解（最多 3 个后台 RootView，而非 N 个）
- **稳定性**：标签上限保护 + 内存警告回收，杜绝 OOM 崩溃

10 个标签的 iPhone 内存对比估算：

| 项目 | 当前 | 优化后 |
|------|------|--------|
| RCTRootView | ~40MB × 10 = 400MB | ~40MB × 3 = 120MB（LRU 缓存上限） |
| 快照内存 | ~4MB × 10 = 40MB | ~0.05MB × 10 = 0.5MB（降采样缩略图） |
| 后台 CPU | N 个 React 渲染循环 | 最多 3 个后台 RootView |
| **总计** | **~440MB** | **~120.5MB** |