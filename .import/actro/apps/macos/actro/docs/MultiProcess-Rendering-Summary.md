# Actro Multi-Process Rendering - 阶段性总结

> 历史实验记录。现行路径见 [Renderer-Process-Pool.md](Renderer-Process-Pool.md)。
>
> **当时分支**: `experiment/viewbridge`
> **当时主攻**: ViewBridge Framework (NSRemoteView)

---

## 本分支实验方向

### ViewBridge Framework (NSRemoteView) - 主攻

**背景**：
ViewBridge 是 Apple 内部用于跨进程视图嵌入的私有框架。macOS 的 Save/Open 对话框、系统偏好设置面板等都使用此框架实现跨进程 UI 显示。

**参考来源**：
- [ViewBridge Framework 实践文章](https://medium.com/@avaidyam/viewbridge-framework-it-works-484de6f31f7d)
- [macOS Headers - NSRemoteView.h](https://github.com/w0lfschild/macOS_headers/blob/master/macOS/PrivateFrameworks/ViewBridge/383/NSRemoteView.h)

**为什么选择这个方向**：
1. Apple 内部大量使用，稳定性有保障
2. 有开发者成功实现："it works, and it works great... loads in less than 300ms"
3. 自动处理事件转发、Accessibility（解决核心诉求）
4. 比 CAContext/CALayerHost 更高层的抽象

**核心类**：
- `NSRemoteView` - 主进程中显示远程视图
- `NSRemoteViewController` - XPC 服务端视图控制器
- `NSRemoteViewMarshal` - 跨进程数据编组
- `NSRemoteViewDelegate` - 生命周期回调

**实现思路**：
```
┌─────────────────────────────────────────────────────────────┐
│                     Main Process (Actro)                    │
│  ┌─────────────────┐    ┌─────────────────────────────────┐ │
│  │   TabManager    │───▶│        NSRemoteView             │ │
│  │                 │    │  (displays remote view via XPC) │ │
│  └─────────────────┘    └─────────────────────────────────┘ │
│           │                          ▲                       │
│           │ XPC (ViewBridge)         │ View Content          │
│           ▼                          │                       │
└───────────┼──────────────────────────┼──────────────────────┘
            │                          │
┌───────────▼──────────────────────────┼──────────────────────┐
│                  Renderer Process (XPC Service)              │
│  ┌─────────────────┐    ┌─────────────────────────────────┐ │
│  │ NSRemoteView-   │───▶│     React Native View           │ │
│  │ Controller      │    │  (renders content in service)   │ │
│  └─────────────────┘    └─────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

**需要研究的关键点**：
1. Info.plist 配置项（XPC 服务需要特定键值）
2. NSRemoteViewController 的正确子类化方式
3. 事件传递机制
4. 与现有 XPC 架构的集成

**预期改动文件**：
- 新增 `Actro/MultiProcess/RemoteViewHost.swift` - NSRemoteView 封装
- 修改 XPC 服务 Info.plist - 添加 ViewBridge 配置
- 可能需要创建新的 XPC 服务 target

---

## 目标

实现 Actro macOS 应用的多进程架构，将 React Native 渲染放在独立进程中执行，主进程通过 IPC 显示渲染结果。

**核心诉求：**
- 进程隔离：React Native 崩溃不影响主进程
- 性能优化：利用多核 CPU，避免主进程阻塞
- 保持 Accessibility 无障碍功能

---

## 已尝试的方案

### 方案 1: IOSurface + CARenderer ❌ 失败

**问题**：
- Metal 对齐崩溃
- 跨进程 IOSurface 查找失败
- 会丢失 Accessibility 功能

### 方案 2: CAContext + CALayerHost ❌ 黑屏

**问题**：
- CALayerHost 无法显示远程内容
- `hasContents=false, sublayers=0`
- 私有 API 行为不明确

---

## 参考资料

1. [ViewBridge Framework 实践](https://medium.com/@avaidyam/viewbridge-framework-it-works-484de6f31f7d)
2. [macOS Headers - ViewBridge](https://github.com/w0lfschild/macOS_headers/tree/master/macOS/PrivateFrameworks/ViewBridge)
3. [NSRemoteView.h](https://github.com/w0lfschild/macOS_headers/blob/master/macOS/PrivateFrameworks/ViewBridge/383/NSRemoteView.h)

---

*最后更新: 2026-01-01*
