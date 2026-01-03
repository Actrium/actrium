# Canvas 浏览器功能讨论

## 2. Tab Overview 页面截图

### 当前状态
- Tab Overview 模式下，Web 内容显示为占位符（globe 图标 + URL）
- 因为 `scaleEffect` 对 `NSViewRepresentable`（WKWebView）不生效

### 实现方案
1. **截图缓存方案**（推荐）
   - 在切换到 Overview 前，调用 `WKWebView.takeSnapshot(with:completionHandler:)`
   - 将截图缓存到内存或磁盘
   - Overview 模式下显示缓存的截图
   - 优点：性能好，实现简单
   - 缺点：截图可能过时

2. **实时缩略图方案**
   - 使用 `CALayer` 的 `render(in:)` 方法
   - 需要额外的渲染层
   - 性能开销较大

### 建议
采用方案1，在以下时机触发截图：
- 从 Focus 切换到 Overview 时
- 页面加载完成时（后台更新）

---

## 3. 下载功能

### 当前状态
- WKWebView 默认不处理下载，需要实现 `WKDownloadDelegate`

### 标准浏览器下载逻辑
1. **检测下载**
   - `WKNavigationDelegate.webView(_:decidePolicyFor:decisionHandler:)` 中检测
   - 响应头 `Content-Disposition: attachment`
   - 或 MIME 类型不可渲染

2. **处理下载**
   - 实现 `WKDownloadDelegate`
   - `download(_:decideDestinationUsing:suggestedFilename:completionHandler:)` 选择保存位置
   - `download(_:didReceive:totalBytesWritten:totalBytesExpectedToWrite:)` 显示进度

3. **UI 组件**
   - 下载管理器窗口
   - 下载进度条
   - 下载历史记录

### 实现优先级
- [ ] 基础下载功能（保存到 Downloads 文件夹）
- [ ] 下载进度显示
- [ ] 下载管理器 UI

---

## 4. 浏览器功能完整性评估

### 已实现功能 ✅
- 多标签页管理
- 前进/后退导航
- 地址栏输入
- 书签系统
- 历史记录
- `target="_blank"` 新标签页
- 自定义 User-Agent
- 开发者工具（DevTools）

### 缺失的核心功能 ❌
| 功能 | 优先级 | 复杂度 |
|------|--------|--------|
| 下载管理 | 高 | 中 |
| 页面截图（Overview） | 高 | 低 |
| 查找功能 (Cmd+F) | 高 | 低 |
| 打印功能 | 中 | 低 |
| 缩放功能 | 中 | 低 |
| Cookie 管理 | 中 | 中 |
| 密码管理 | 低 | 高 |
| 扩展系统 | 低 | 高 |
| 阅读模式 | 低 | 中 |
| 全屏模式 | 低 | 低 |

### 多进程模型对比

#### 当前 Canvas 实现
```
Canvas App (主进程)
    └── WKWebView
            └── WebContent Process (由 WebKit 管理)
                    └── 共享 WKProcessPool
```

- 使用 `WKProcessPool` 共享进程池
- WebKit 自动管理 WebContent 进程
- 沙盒隔离由系统处理

#### Safari 多进程架构
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

#### 结论
Canvas 使用的是 **WKWebView 标准多进程模型**，与 Safari 的底层实现一致（都基于 WebKit），但：
- Safari 有更严格的站点隔离策略
- Safari 有独立的网络/GPU 进程
- Canvas 的进程管理完全由 WebKit 框架处理，是"标准 Safari 同款"的简化版

### 建议优先实现
1. **页面截图** - 提升 Overview 体验
2. **下载功能** - 基础浏览器必需
3. **查找功能** - 用户常用功能
