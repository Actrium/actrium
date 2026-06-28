# askit Documentation

Welcome to the askit documentation.

**askit** is a UI component library and API layer built on top of [rill](https://github.com/GoAskAway/rill). Through package.json conditional exports, the same `import { StepList, Toast } from 'askit'` delivers real React Native components on the Host side and string identifiers on the Guest side (which rill passes to Host for rendering).

| Layer | Role |
|-------|------|
| **rill** | Sandbox-isolated dynamic UI rendering engine — runs React code in isolated JS sandbox (QuickJS/JSC), serializes render operations into instructions, sends them to Host for real React Native rendering |
| **askit** | UI components and API layer on top of rill — provides business components (StepList, ChatBubble, etc.) and cross-boundary APIs (EventEmitter, Toast, Haptic) |

## Table of Contents

- [Getting Started](./getting-started.md)
- [Integration Guide](./integration.md) - **askit & rill collaboration**
- [API Reference](./api-reference.md)
- [Components](./components.md)
- [Best Practices](./best-practices.md)
- [Architecture](./architecture.md)

## Quick Links

- [GitHub Repository](https://github.com/GoAskAway/askit)
- [Issue Tracker](https://github.com/GoAskAway/askit/issues)
