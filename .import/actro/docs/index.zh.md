# askit 文档

欢迎阅读 askit 文档。

**askit** 是构建在 [rill](https://github.com/GoAskAway/rill) 之上的 UI 组件库与 API 层。通过 package.json 条件导出，同一份 `import { StepList, Toast } from 'askit'` 在 Host 端获得真实 React Native 组件，在 Guest 端获得字符串标识符（由 rill 传递给 Host 渲染）。

| 层级 | 职责 |
|------|------|
| **rill** | 沙箱隔离的动态 UI 渲染引擎 — 在独立的 JS 沙箱（QuickJS/JSC）中运行 React 代码，将渲染操作序列化为指令发送给 Host，由 Host 端的真实 React Native 执行渲染 |
| **askit** | 构建在 rill 之上的 UI 组件与 API 层 — 提供业务组件（StepList、ChatBubble 等）和跨边界 API（EventEmitter、Toast、Haptic） |

## 目录

- [快速开始](./getting-started.md)
- [集成指南](./integration.md) - **askit 与 rill 协作**
- [API 参考](./api-reference.md)
- [组件](./components.md)
- [最佳实践](./best-practices.md)
- [架构](./architecture.md)

## 快速链接

- [GitHub 仓库](https://github.com/GoAskAway/askit)
- [问题追踪](https://github.com/GoAskAway/askit/issues)
