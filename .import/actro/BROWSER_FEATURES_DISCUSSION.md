# Canvas 浏览器功能讨论

## 1. Sidebar Favicon ✅ 已实现

### 实现方案
- Web tabs 使用 Google Favicon 服务加载图标
- 使用 actor-based `FaviconCache` 进行线程安全的内存缓存
- 非 Web tabs 显示默认 doc.text 图标

### 代码位置
- `TabOutlineView.swift:makeTabRowView()`
- `FaviconCache` actor

---

## 2. Tab Overview 页面截图 ✅ 已实现

### 实现方案
- 使用 `WKWebView.takeSnapshot()` API 捕获页面截图
- 截图缓存在 `WebViewHolder.snapshot` 属性中
- 触发时机：
  - 从 Focus 切换到 Overview 时
  - 页面加载完成后（延迟 0.5s 等待渲染）
- Overview 模式下显示缓存的截图，无截图时显示占位符

### 代码位置
- `WebViewTabView.swift:WebViewHolder.takeSnapshot()`
- `WebViewTabView.swift:WebViewMultiProcessTabView`
- `WorkspaceMainView.swift:onChange(of: viewModel.canvasMode)`

---

## 3. 下载功能 ✅ 已实现

### 实现方案
- 实现 `WKDownloadDelegate` 协议
- 通过 `WKNavigationDelegate.webView(_:decidePolicyFor:)` 检测下载
- 检测条件：
  - `Content-Disposition: attachment` 响应头
  - 不可渲染的 MIME 类型（zip, dmg, pdf 等）
- 下载保存到 `~/Downloads`，自动处理重名文件
- 下载完成后自动在 Finder 中显示

### 代码位置
- `WebViewTabView.swift:WebViewHolder` (WKDownloadDelegate 实现)

### 待改进
- [ ] 下载进度 UI
- [ ] 下载管理器窗口
- [ ] 下载历史记录

---

## 4. 查找功能 (Cmd+F) ✅ 已实现

### 实现方案
- 使用 `WKWebView.find(_:configuration:)` API
- 仅在 Focus 模式下的 Web 标签页可用
- 快捷键：
  - `Cmd+F`: 打开/关闭查找栏
  - `Escape`: 关闭查找栏
  - `Enter`: 查找下一个
- 实时搜索（边输入边查找）
- 上一个/下一个导航按钮

### 代码位置
- `WorkspaceMainView.swift:FindInPageBar`
- `WorkspaceMainView.swift:performFind()`

---

## 5. 浏览器功能完整性评估

### 已实现功能 ✅
- 多标签页管理
- 前进/后退导航
- 地址栏输入
- 书签系统
- 历史记录
- `target="_blank"` 新标签页
- 自定义 User-Agent
- 开发者工具（DevTools）
- **Sidebar Favicon（带缓存）**
- **Tab Overview 页面截图**
- **下载功能（基础）**
- **查找功能 (Cmd+F)**

### 待实现功能
| 功能 | 优先级 | 复杂度 |
|------|--------|--------|
| 下载管理器 UI | 中 | 中 |
| 打印功能 | 中 | 低 |
| 缩放功能 | 中 | 低 |
| Cookie 管理 | 中 | 中 |
| 密码管理 | 低 | 高 |
| 扩展系统 | 低 | 高 |
| 阅读模式 | 低 | 中 |
| 全屏模式 | 低 | 低 |

---

## 6. 多进程模型对比

### 当前 Canvas 实现
```
Canvas App (主进程)
    └── WKWebView
            └── WebContent Process (由 WebKit 管理)
                    └── 共享 WKProcessPool
```

- 使用 `WKProcessPool` 共享进程池
- WebKit 自动管理 WebContent 进程
- 沙盒隔离由系统处理

### Safari 多进程架构
```
Safari (UI Process)
    ├── WebContent Process 1 (Tab 1)
    ├── WebContent Process 2 (Tab 2)
    ├── Networking Process
    └── GPU Process
```

Safari 的特点：
- 每个标签页独立进程（站点隔离）
- 专用网络进程
- GPU 进程分离
- 更细粒度的沙盒

### 结论
Canvas 使用的是 **WKWebView 标准多进程模型**，与 Safari 的底层实现一致（都基于 WebKit），但：
- Safari 有更严格的站点隔离策略
- Safari 有独立的网络/GPU 进程
- Canvas 的进程管理完全由 WebKit 框架处理，是"标准 Safari 同款"的简化版
