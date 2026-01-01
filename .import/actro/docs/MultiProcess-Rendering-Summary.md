# Canvas Multi-Process Rendering - 阶段性总结

## 目标

实现 Canvas macOS 应用的多进程架构，将 React Native 渲染放在独立进程中执行，主进程通过 IPC 显示渲染结果。

**核心诉求：**
- 进程隔离：React Native 崩溃不影响主进程
- 性能优化：利用多核 CPU，避免主进程阻塞
- 保持 Accessibility 无障碍功能

---

## 架构设计

```
┌─────────────────────────────────────────────────────────────┐
│                     Main Process (Canvas)                    │
│  ┌─────────────────┐    ┌─────────────────────────────────┐ │
│  │   TabManager    │───▶│      IOSurfaceHostView          │ │
│  │                 │    │  (displays remote CALayerHost)  │ │
│  └─────────────────┘    └─────────────────────────────────┘ │
│           │                          ▲                       │
│           │ XPC                      │ contextId             │
│           ▼                          │                       │
└───────────┼──────────────────────────┼──────────────────────┘
            │                          │
┌───────────▼──────────────────────────┼──────────────────────┐
│                  Renderer Process (CanvasTabRenderer)        │
│  ┌─────────────────┐    ┌─────────────────────────────────┐ │
│  │ TabRendererSvc  │───▶│  CAContext + React Native View  │ │
│  │                 │    │  (renders content, shares layer)│ │
│  └─────────────────┘    └─────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

---

## 尝试过的方案

### 方案 1: IOSurface + CARenderer ❌ 失败

**实现思路：**
- 渲染进程使用 `CARenderer` 将 React Native 视图渲染到 `IOSurface`
- 通过 Mach port 跨进程共享 IOSurface
- 主进程使用 Metal 显示 IOSurface 内容

**遇到的问题：**
1. **Metal 对齐崩溃**：`failed assertion !fbAttachment.getTexture(AttachmentType::Normal)->isMisalignedIOSurface()`
   - IOSurface 的内存对齐不符合 Metal 要求

2. **跨进程 IOSurface 查找失败**：`IOSurfaceLookup failed for ID`
   - 需要 Mach port 传递而非简单的 ID 传递

3. **同学建议（来自隔壁团队）**：
   - CARenderer 是 CPU/GPU 密集型操作，会造成性能问题
   - 会丢失 Accessibility 功能
   - 对于 React Native 场景是一个"坑"

---

### 方案 2: CAContext + CALayerHost (私有 API) 🔄 当前尝试

**实现思路：**
- 渲染进程使用 `CAContext` 创建可跨进程共享的 layer context
- 将 React Native 视图的 layer 赋给 `CAContext.layer`
- 主进程使用 `CALayerHost` 通过 contextId 显示远程内容

**参考来源：**
- Chromium: `ui/accelerated_widget_mac/`
- WebKit: `WebCore/platform/graphics/cocoa/LayerHostingContext.mm`
- Electron: 使用相同模式

**已实现：**
1. ✅ CAContext 创建成功（使用 `remoteContextWithOptions:`）
2. ✅ contextId 通过 XPC 传递到主进程
3. ✅ React Native 内容加载成功（`RCTContentDidAppearNotification` 触发）
4. ✅ 渲染进程 layer tree 有内容（sublayers=1）
5. ✅ Layer-Hosting View 模式（先创建 CALayer 再设置 wantsLayer）
6. ✅ 禁用所有隐式动画

**当前问题：**
- ❌ CALayerHost 显示黑屏
- ❌ `hasContents=false, sublayers=0`
- ❓ `connected` 属性状态未知

---

## 关键代码片段

### 渲染进程 - CAContext 创建

```swift
// CARemoteLayer.swift
var contextOptions: [String: Any] = options ?? [:]
contextOptions["CIFilterBehavior"] = "ignore"  // WebKit's kCAContextCIFilterBehavior

// Try remoteContextWithOptions: first
let remoteSelector = NSSelectorFromString("remoteContextWithOptions:")
if let ctx = factory(contextClass, remoteSelector, contextOptions) {
    self.context = ctx
}
```

### 渲染进程 - Layer-Hosting 模式

```swift
// TabRendererService.swift
// Layer-Hosting pattern: Create layer BEFORE wantsLayer
let rootLayer = CALayer()
rootLayer.actions = [
    "bounds": NSNull(),
    "position": NSNull(),
    "contents": NSNull(),
    "sublayers": NSNull(),
]
container.layer = rootLayer  // Set BEFORE wantsLayer
container.wantsLayer = true
```

### 主进程 - CALayerHost

```swift
// IOSurfaceHostView.swift
let layerHostClass: AnyClass = NSClassFromString("CALayerHost")!
let hostLayer = (layerHostClass as! CALayer.Type).init()
hostLayer.setValue(contextId, forKey: "contextId")
```

---

## 当前卡点分析

### 可能原因 1: CAContext 类型不对

WebKit 使用 `contextWithCGSConnection:options:` 而非 `remoteContextWithOptions:`

```objc
// WebKit LayerHostingContext.mm
CGSConnectionID
auto context = [CAContext contextWithCGSConnection:CGSMainConnectionID() options:@{
    kCAContextCIFilterBehavior: @"ignore",
    kCAContextDisplayId: @(displayId),
    kCAContextIgnoresHitTest: @YES,
}];
```

### 可能原因 2: 缺少关键选项

WebKit 额外使用了：
- `kCAContextDisplayId` - 显示器 ID
- `kCAContextIgnoresHitTest` - 忽略点击测试
- `kCAContextSecure` - 安全模式（仅在 iOS）

### 可能原因 3: Layer 同步时机

可能需要在设置 CAContext.layer 后调用：
- `invalidateFences()` - 强制 fence 同步
- `CATransaction.flush()` - 强制事务提交

### 可能原因 4: 进程权限/沙盒问题

XPC 服务可能需要特殊 entitlements 才能跨进程共享 layer

---

## 备选方案 (未实施)

### 方案 3: NSWindow 子窗口

**思路：**
- 渲染进程创建 offscreen NSWindow
- 主进程使用 `addChildWindow:ordered:` 嵌入
- 依赖系统的窗口合成

**优点：**
- 更标准的系统 API
- 可能更稳定

**缺点：**
- 窗口层级管理复杂
- 可能有性能开销

---

## 下一步计划

1. **深入研究 Chromium 源码**
   - 查看 `ui/accelerated_widget_mac/ca_renderer_layer_tree.mm`
   - 理解完整的 CAContext 初始化流程

2. **验证 connected 属性**
   - 确认 CALayerHost 是否真正连接到远程 context

3. **尝试 contextWithCGSConnection**
   - 使用 WebKit 的 CGS connection 方式创建 context

4. **检查 entitlements**
   - 确认 XPC 服务有正确的权限配置

---

## 相关文件

| 文件 | 作用 |
|------|------|
| `CanvasTabRenderer/TabRendererService.swift` | 渲染进程服务，创建 CAContext |
| `CanvasTabRenderer/CARemoteLayer.swift` | CAContext/CALayerHost 私有 API 封装 |
| `Canvas/MultiProcess/IOSurfaceHostView.swift` | 主进程显示视图 |
| `Canvas/MultiProcess/TabRendererClient.swift` | XPC 客户端 |
| `Canvas/MultiProcess/TabRendererProxy.swift` | XPC 代理 |

---

## 参考资料

1. [Chromium CALayerHost usage](https://chromium.googlesource.com/chromium/src/+/HEAD/ui/accelerated_widget_mac/)
2. [WebKit LayerHostingContext](https://github.com/nickhutchinson/WebKit/blob/main/Source/WebCore/platform/graphics/cocoa/LayerHostingContext.mm)
3. [iOS Runtime Headers - CALayerHost](https://github.com/nst/iOS-Runtime-Headers/blob/master/Frameworks/QuartzCore.framework/CALayerHost.h)

---

*最后更新: 2026-01-01*
