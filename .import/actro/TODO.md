# Canvas 项目 TODO

## 当前状态 (2026-02-28)

### 已完成事项

#### 1. 窗口调整与 WebView 联动修复
- ✅ 修复了双击标题栏全屏时，内容区域 (WebView/RN View) 未能跟随窗口大小变化的问题
- ✅ 在 `ViewBridgeHostView.swift` 中优化了 `setFrameSize` 逻辑，去除了过于严格的像素级检查
- ✅ 使用 `CATransaction.setDisableActions(true)` 禁用了 Layer 隐式动画，实现一步到位的大小调整
- ✅ 验证通过：窗口大小变化时，WebView/RN 内容区域实时同步

#### 2. 背景色自适应功能（Safari/Arc 风格）
- ✅ **Web 页面背景色提取**：
  - 在 `WebViewTabView.swift` 中通过 JavaScript 提取当前页面背景色
  - 支持读取 `<meta name="theme-color">`、`document.body` 和 `document.documentElement` 的背景色
  - 通过 `WKScriptMessageHandler` 将颜色传递回 Swift
  - 使用 `BrowserViewModel.updateTabBackgroundColor()` 更新 Tab 的 `backgroundColor` 字段
- ✅ **ACTR 应用背景色提取**：
  - 在 `BrowserViewModel.readManifestJSON()` 中读取 `manifest.json` 的 `themeColor` 或 `backgroundColor` 字段
  - 在创建 ACTR Tab 时将颜色设置到 Tab 的 `backgroundColor` 属性
- ✅ **窗口背景色动态更新**：
  - 在 `ContentView.swift` 的 `WindowObserverView` 中监听 `viewModel.currentTab?.backgroundColor` 变化
  - 使用 `NSColor(hexString:)` 将 Hex 颜色转换为 `NSColor`
  - 动态更新 `window.backgroundColor`，使侧边栏透明材质显示当前 Tab 的背景色
- ✅ **验证结果**：
  - **Web 页面**（如百度首页）：白色背景正常提取并应用到窗口背景 ✅
  - **ACTR 应用**（如 helloworld）：背景色提取失败，未能生效 ❌

#### 3. Overview 模式边框问题
- ✅ 移除了 `ViewBridgeHostView` 和 `ContainerView` 中的调试背景色（红色边框）
- ✅ 验证：Overview 模式不再出现红色边框

#### 4. Arc 风格布局重构（部分完成）
- ✅ **ContentView 布局调整**：
  - 使用 `HStack` 替代 `NavigationSplitView`，实现自定义的侧边栏 + 主内容区布局
  - 侧边栏宽度可调节（手势拖拽），最小宽度 78px（折叠），最大 400px
  - 主内容区 (`WorkspaceView`) 包裹在圆角卡片中，带阴影效果
- ✅ **侧边栏样式优化**：
  - `SidebarView` 背景透明，与窗口背景融合
  - `TabOutlineView` 强制透明背景
- ✅ **ArcStyleToolbar 组件创建**：
  - 在 `WorkspaceMainView.swift` 中创建了 `ArcStyleToolbar` 结构体
  - 工具栏布局：[侧边栏切换 | ← → | 刷新 | 地址栏 | 分享 | + | Overview]
  - 地址栏样式仿 Arc（默认显示域名，点击进入编辑模式）
  - 添加了 `AddressInputField` 组件（NSViewRepresentable）
- ✅ **原生 Toolbar 移除**：
  - 清空了 `WorkspaceMainView` 的 `.toolbar {}` 内容
  - 工具栏逻辑改为内嵌在 Focus 内容区域顶部
- ✅ **TitlebarManager 更新**：
  - 添加了 `toggleSidebarVisibility()` 公开方法，供 SwiftUI 调用

---

## 当前问题

### 1. **Arc 风格布局未生效** ⚠️ **最高优先级**
**症状**：
- 截图显示当前界面仍然使用原生 Toolbar（顶部有独立的工具栏）
- 与 Arc 参考图差距明显：
  - Arc：主内容区是一张独立卡片，工具栏在卡片内部顶部
  - 当前：主内容区仍然是传统布局，工具栏在窗口顶部

**可能原因**：
1. **`useNativeToolbar` 标志未正确设置**：
   - `ArcStyleToolbar` 的显示依赖 `!useNativeToolbar` 条件
   - 如果 `useNativeToolbar` 为 `true`，则 `ArcStyleToolbar` 不会渲染
   - 需要检查 `WorkspaceMainView` 中 `useNativeToolbar` 的值（来自 `CanvasConfig.useNativeToolbar`）
2. **应用未重新编译或缓存问题**：
   - `xcodebuild` 虽然成功，但可能未完全重启应用进程
   - SwiftUI 视图可能存在缓存
3. **Focus 模式未触发**：
   - `ArcStyleToolbar` 只在 **Focus 模式**下显示（`if case .focus`）
   - 如果当前是 **Overview 模式**，则看不到 Toolbar
   - 截图显示的是 "about:blank" 的 Start Page，可能处于 Overview 模式？

**待排查**：
- [ ] 确认 `CanvasConfig.useNativeToolbar` 的值（是否默认 `true`）
- [ ] 确认当前 `canvasMode` 是 `.focus` 还是 `.overview`
- [ ] 强制 Kill 应用进程，重新编译运行

### 2. **ACTR 应用背景色提取失败** ⚠️
**症状**：
- 打开 `askw:///Users/example-dev-2/.../helloworld` 时，背景色未能从 `manifest.json` 中读取
- 日志显示 `从 manifest 读取 themeColor: #FF0000`，但最终未生效

**可能原因**：
1. **`appendACTRTab` 参数传递问题**：
   - `loadFromDevDirectory` 调用 `appendACTRTab(themeColor: manifestInfo.themeColor)` 时，`themeColor` 可能为 `nil`
   - 或者 `Tab` 初始化时未正确设置 `backgroundColor` 字段
2. **`manifest.json` 解析问题**：
   - `helloworld/manifest.json` 的 `themeColor` 或 `backgroundColor` 字段拼写错误或格式不正确
   - JSON 解析逻辑可能遗漏了该字段
3. **Tab 切换时未触发 `backgroundColor` 更新**：
   - `WindowObserverView` 的 Combine 订阅可能未正确触发
   - `viewModel.tabs` 数组修改后，`currentTab` 计算属性未触发 `objectWillChange`

**待排查**：
- [ ] 查看 `helloworld/manifest.json` 内容，确认 `themeColor` 字段是否存在且格式正确
- [ ] 在 `appendACTRTab` 中添加日志，打印 `themeColor` 参数值
- [ ] 在 `WindowObserverView` 的 `updateWindowBackgroundColor` 中添加日志，确认是否被调用

### 3. **部分 Web 页面背景色提取失败**
**症状**：
- 某些网页（如 `https://baijiahao.baidu.com/...`）背景色未能正确提取

**可能原因**：
1. **JavaScript 执行时机问题**：
   - 页面尚未完全加载时执行 JS，导致 `document.body` 或 `documentElement` 背景色为空
   - `readyState === 'complete'` 检查可能不够
2. **页面背景色透明**：
   - 某些页面 `body` 和 `html` 的背景色均为 `transparent`，浏览器默认渲染白色
   - 当前 JS 逻辑检测到透明后返回空，未提供兜底逻辑
3. **CSP 限制**：
   - 部分网站的 Content Security Policy 可能阻止 `WKUserScript` 执行

**待优化**：
- [ ] 在 JavaScript 中添加更完善的兜底逻辑（如检测到透明时默认返回 `#FFFFFF`）
- [ ] 添加更详细的日志，记录每个步骤的检测结果

---

## TODO 列表

### 优先级 P0（紧急，阻塞 Arc 风格布局）

- [ ] **排查 `useNativeToolbar` 标志**：
  - 在 `CanvasConfig.swift` 中确认 `useNativeToolbar` 的默认值
  - 如果为 `true`，改为 `false` 以启用 `ArcStyleToolbar`
  - 重新编译并验证

- [ ] **确认 Focus 模式触发逻辑**：
  - 在 `WorkspaceMainView` 的 `body` 中添加日志，打印当前 `canvasMode`
  - 确认点击 Tab 后是否进入 `.focus` 模式
  - 如果未进入 Focus 模式，排查 `enterFocus` 逻辑

- [ ] **强制 Kill 应用进程**：
  - 使用 `killall Canvas` 或 Activity Monitor 完全终止应用
  - 清理 DerivedData（如有必要）
  - 重新 `xcodebuild` 并运行

- [ ] **验证 ArcStyleToolbar 渲染**：
  - 在 `ArcStyleToolbar` 的 `body` 中添加明显的视觉标识（如红色背景）
  - 确认 Toolbar 是否被渲染但被遮挡，或完全未渲染

### 优先级 P1（重要，功能完善）

- [ ] **修复 ACTR 应用背景色提取**：
  - 检查 `helloworld/manifest.json` 的 `themeColor` 字段
  - 在 `appendACTRTab` 中添加日志，确认 `themeColor` 参数传递正确
  - 验证 `Tab` 初始化时 `backgroundColor` 字段是否正确赋值
  - 测试其他 ACTR 应用（如有）

- [ ] **完善 Web 页面背景色提取**：
  - 在 JavaScript 中添加兜底逻辑：
    - 如果 `body` 和 `html` 均透明，默认返回 `#FFFFFF`（浅色模式）或 `#1E1E1E`（深色模式）
  - 添加 `window.matchMedia('(prefers-color-scheme: dark)')` 检测
  - 优化 JavaScript 执行时机（考虑 `DOMContentLoaded` 或延迟执行）

- [ ] **优化 WindowObserverView 日志**：
  - 在 `updateWindowBackgroundColor` 中添加详细日志：
    - 打印 `currentTab?.backgroundColor` 的值
    - 打印转换后的 `NSColor` 对象
    - 确认 `window.backgroundColor` 是否成功更新

### 优先级 P2（次要，体验优化）

- [ ] **Arc 风格细节调整**：
  - 地址栏样式微调（圆角、背景色、Hover 效果）
  - Toolbar 按钮间距、图标大小与 Arc 对齐
  - 卡片阴影和圆角参数优化

- [ ] **侧边栏拖拽体验优化**：
  - 拖拽 Divider 时，Cursor 样式改为 `resizeLeftRight`
  - 拖拽时显示半透明 Divider 预览
  - 释放时有弹性动画

- [ ] **Overview 模式 Toolbar 显示**：
  - 当前 `ArcStyleToolbar` 只在 Focus 模式显示
  - Arc 在 Overview 模式也有简化的 Toolbar
  - 考虑在 Overview 模式显示精简版 Toolbar（只保留 + 和 Overview 切换按钮）

- [ ] **测试 ACTR 应用左侧面板功能**：
  - 验证 `toggleLeftPanel` 在 ACTR 应用中是否正常工作
  - 确认 XPC 消息传递是否正确

---

## 技术债务

### 1. **代码重复**
- `CustomTitlebar.swift` 中的 `LiquidGlassToolbar` 和 `WorkspaceMainView.swift` 中的 `ArcStyleToolbar` 功能重复
- 如果确认不再使用原生 Toolbar，可以考虑删除 `CustomTitlebar.swift` 的相关逻辑

### 2. **日志清理**
- 当前代码中有大量调试日志（`NSLog`、`logger.notice`、`logger.debug`）
- 功能稳定后，应清理或使用统一的日志框架

### 3. **类型安全**
- `Tab.backgroundColor` 字段使用 `String` 类型存储 Hex 颜色
- 考虑使用 `Color` 或自定义 `HexColor` 类型，避免运行时解析失败

---

## 下一步建议

1. **立即排查 Arc 布局未生效的原因**（P0）：
   - 检查 `useNativeToolbar` 配置
   - 确认 Focus 模式是否触发
   - Kill 应用进程并重新编译

2. **修复 ACTR 背景色问题**（P1）：
   - 验证 `manifest.json` 解析
   - 确认 `Tab` 初始化逻辑

3. **完善日志记录**（P1）：
   - 在关键路径添加详细日志，便于后续调试

4. **优化用户体验**（P2）：
   - 地址栏交互细节
   - 侧边栏拖拽体验

---

## 参考资料

- **Arc 浏览器截图**：已提供，主要特征：
  - 侧边栏固定在左侧（深色背景）
  - 主内容区是独立的圆角卡片，浮动在右侧
  - 工具栏在卡片内部顶部（与内容一体）
  - 无传统的窗口级 Toolbar

- **Safari 浏览器**：
  - 侧边栏透明材质，融合窗口背景
  - 工具栏在窗口顶部（独立于内容区）

- **当前 Canvas 实现**：
  - 目标：Arc 风格（卡片 + 内嵌 Toolbar）
  - 现状：仍显示原生 Toolbar，未达成目标

---

## 开发环境信息

- **macOS 版本**：26.1+
- **Xcode 版本**：17C529
- **Swift 版本**：Swift 6
- **项目路径**：`/Users/example-dev-2/Projects/example-workspace/GoAskAway/Canvas`
- **构建路径**：`/Users/example-dev-2/Library/Developer/Xcode/DerivedData/Canvas-bfrmsauxlmtqwybyzrckiznxvqhp/Build/Products/Debug/Canvas.app`

---

**最后更新时间**：2026-02-28 16:56 UTC
