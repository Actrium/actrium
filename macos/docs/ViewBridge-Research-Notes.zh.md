# ViewBridge.framework 研究笔记

## 1. 关键参考项目

### 1.1 avaidyam/RemoteViewHost (⭐⭐⭐⭐⭐ 必看)
- **仓库**: https://github.com/avaidyam/RemoteViewHost
- **描述**: 使用 ViewBridge.framework 将 GUI 代码分离到不同服务的 PoC
- **语言**: Objective-C (93.5%) + Swift (6.5%)
- **核心结构**:
  - `RemoteViewHost/` - 主 Host 应用
  - `RemoteViewService/` - XPC Service 组件
  - `ViewBridgePublic/` - 公开的 ViewBridge 头文件

### 1.2 avaidyam 博客文章
- **URL**: https://medium.com/@avaidyam/viewbridge-framework-it-works-484de6f31f7d
- **关键信息**:
  - 创建 XPC service + 添加几个 Info.plist 键 + 获取头文件即可工作
  - 服务视图加载时间 < 300ms
  - 可以定义自定义 NSExtensionSDK 来使其"可扩展"（类似 Today Widgets）

### 1.3 macOS Headers (私有头文件)
- **仓库**: https://github.com/w0lfschild/macOS_headers
- **关键路径**: `macOS/PrivateFrameworks/ViewBridge/383/`
- **重要文件**:
  - `NSRemoteView.h`
  - `NSRemoteViewController.h`
  - `NSRemoteViewDelegate-Protocol.h`
  - `NSRemoteViewMarshal.h`

### 1.4 WebKit (祖师爷级别)
- **仓库**: https://github.com/WebKit/WebKit
- **关键路径**: `Source/WebKit/UIProcess/Cocoa/`
- **相关文件**: `WKWebView.mm`, `WKWebViewIOS.mm`

---

## 2. NSRemoteView 关键接口

### 2.1 类定义
```objc
@interface NSRemoteView : NSView <
    NSXPCListenerDelegate,
    NSVBXPCConnectionClient,
    HasAuxiliaryConnection,
    AuxiliaryCallsHost,
    _NSAlertSensitiveView
>
```

### 2.2 Resize 相关方法
```objc
// Service 请求 resize（带动画和完成回调）
- (void)serviceRequestsResize:(struct CGSize)arg1
    serviceWindowBackgroundColor:(id)arg2
    safeFrame:(struct CGRect)arg3
    animate:(BOOL)arg4
    transaction:(id)arg5
    completion:(CDUnknownBlockType)arg6;

// 简化版本
- (void)_serviceRequestsResize:(struct CGSize)arg1
    animate:(BOOL)arg2
    completion:(CDUnknownBlockType)arg3;

- (void)_serviceRequestsResize:(struct CGSize)arg1
    completion:(CDUnknownBlockType)arg2;
```

### 2.3 关键属性
- `preferredContentSize` - 期望的内容尺寸
- `allowsImplicitResizeRequests` - 是否允许隐式 resize 请求

---

## 3. WebKit 的 Resize 处理策略 (⭐⭐⭐⭐⭐ 核心)

### 3.1 不信任自动 Resize
WebKit **从来不信任** ViewBridge 的自动 Resize。

### 3.2 官方做法
1. **Host 端 (UIProcess)** 在 Resize 时，主动发一条 IPC 消息给 WebProcess
2. 消息内容："我要变大了，你赶紧重新 Layout"
3. **冻结当前画面 (Snapshot)**，直到 WebProcess 回复"我画好了"
4. 然后切换显示

### 3.3 代码证据
```
// WKWebView.mm 中的日志
"Stopped holding window resize snapshot for window full screen"
```

### 3.4 iOS 实现细节 (WKWebViewIOS.mm)
- 使用 `RemoteLayerTreeDrawingAreaProxy` 追踪 transaction ID
- `drawingArea->setSize()` 设置绘图区域尺寸
- 动态视口更新模式:
  - `ResizingWithAnimation`
  - `ResizingWithDocumentHidden`
- **Live resize 快照管理**: 如果网页无响应，1秒后强制移除快照

---

## 4. 关键问题与解决方案

### 4.1 问题：SwiftUI scaleEffect 导致 Resize Storm
**原因**: SwiftUI 的 scaleEffect 会通过 CALayer transform 放大视图，但 NSRemoteView 的内容是独立进程渲染的，不受 transform 影响。

**表现**:
- 画布被 scaleEffect 放大 (如 2.5x)
- ViewBridgeHostView 的 frame 被放大
- XPC Service 不知道这个 transform，导致尺寸不匹配

### 4.2 解决方案思路

#### 方案 A: 截断布局传递 (推荐)
在 `NSViewRepresentable` 内部截断 SwiftUI 的布局传递：
```swift
// 不要让 SwiftUI 直接管理 NSRemoteView 的 Frame
// 用纯 AppKit 的 NSView 去包裹它
```

#### 方案 B: 逆向 Transform 抵消
```swift
// Focus 模式下，用 1/currentScale 来抵消画布放大
let scaleFactor = isFocused ? (1.0 / currentScale) : (cardWidth / viewportSize.width)
```

#### 方案 C: 纯 AppKit 实现 (WebKit 的做法)
- 不在被 scaleEffect 影响的视图层级中放置 ViewBridge
- 将 ViewBridge 内容放在画布之外，直接覆盖在视口上

### 4.3 尺寸同步的正确流程
1. Host 端计算目标尺寸
2. 发送 IPC 消息通知 Service 端
3. **冻结当前画面** (可选：显示快照)
4. 等待 Service 端完成渲染
5. Service 端回复"完成"
6. 切换显示新内容

---

## 5. Info.plist 配置

### 5.1 XPC Service 端
```xml
<key>NSExtension</key>
<dict>
    <key>NSExtensionPointIdentifier</key>
    <string>com.apple.ViewBridge</string>
</dict>

<key>XPCService</key>
<dict>
    <key>RunLoopType</key>
    <string>_NSApplicationMain</string>
</dict>

<key>NSSupportsViewBridge</key>
<true/>
```

---

## 6. TODO

- [ ] 克隆 avaidyam/RemoteViewHost 研究具体实现
- [ ] 研究 WebKit 的 `_pageExtendedBackgroundColorDidChange` 和 resize snapshot 机制
- [ ] 尝试纯 AppKit 方案：不让 SwiftUI 管理 NSRemoteView 的 frame
- [ ] 实现 IPC 消息通知机制，让 Host 主动告知 Service 尺寸变化
- [ ] 考虑实现 Snapshot 机制避免 resize 闪屏

---

## 7. 参考链接

- [RemoteViewHost PoC](https://github.com/avaidyam/RemoteViewHost)
- [ViewBridge.framework Wiki](https://github.com/avaidyam/Parrot/wiki/ViewBridge.framework)
- [macOS Private Headers](https://github.com/w0lfschild/macOS_headers)
- [WebKit WKWebView.mm](https://github.com/WebKit/webkit/blob/main/Source/WebKit/UIProcess/API/Cocoa/WKWebView.mm)
- [Medium 文章](https://medium.com/@avaidyam/viewbridge-framework-it-works-484de6f31f7d)
