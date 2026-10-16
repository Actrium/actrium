# React Native Bundle 构建指南

Actrium 项目包含独立的 React Native bundle。现行 Helper 是单一 `ActriumRenderer.app`，按 fingerprint 隔离进程，不再复制 `RCTViewService*.xpc`。详见 [Renderer-Process-Pool.md](Renderer-Process-Pool.md)。

| Bundle | 入口文件 | 输出文件 | 用途 |
|--------|----------|----------|------|
| System Apps | `index-system.tsx` | `system.jsbundle` | 系统 Renderer（`system.*`） |
| Actr Host | `index-runtime.tsx` | `runtime.jsbundle` | 普通应用 Renderer |

## 快速使用

```bash
# 构建所有 bundle（Release 模式）
./scripts/bundle-actrium-runtime.sh

# 仅构建 System Apps bundle
./scripts/bundle-actrium-runtime.sh --target system

# 仅构建 Actr Host bundle
./scripts/bundle-actrium-runtime.sh --target actr

# 构建 Development bundle（包含 source map）
./scripts/bundle-actrium-runtime.sh --dev

# 查看帮助
./scripts/bundle-actrium-runtime.sh --help
```

## Bundle 说明

### System Apps (`system.jsbundle`)

**用途**：由 RCTViewService0 加载，渲染系统内置页面。

**包含的应用**：
- `StartPageApp` - 新标签页（`about:blank`）
- `HistoryApp` - 历史记录（`about:history`）
- `BookmarksApp` - 书签管理（`about:bookmarks`）
- `DebugApp` - 调试页面（`about:debug`）

**入口文件**：`packages/actrium/index-system.tsx`

### Actr Host (`runtime.jsbundle`)

**用途**：由 RCTViewService1-N 加载，渲染 `.actr` 插件应用。

**包含的功能**：
- `RuntimeApp` - 插件宿主环境
- `keel` / `askit` 引擎支持
- Guest sandbox 运行时

**入口文件**：`packages/actrium/index-runtime.tsx`

## 源码结构

```
packages/actrium/
├── index-system.tsx      # System Apps 入口
├── index-runtime.tsx        # Actr Host 入口
├── index.ts              # Legacy 统一入口（已废弃）
└── src/
    ├── system-apps/      # 系统应用源码
    │   ├── StartPageApp.tsx
    │   ├── HistoryApp.tsx
    │   ├── BookmarksApp.tsx
    │   └── DebugApp.tsx
    ├── runtime-host/        # Actr 宿主源码
    │   ├── RuntimeApp.tsx
    │   └── BundleView.tsx
    ├── TabRouter.tsx     # 路由器（Legacy）
    └── logger.ts         # 日志工具
```

## 开发流程

1. **修改源码**：编辑 `packages/actrium/src/` 下的文件

2. **构建 bundle**：
   ```bash
   ./scripts/bundle-actrium-runtime.sh
   ```

3. **构建 Xcode 项目**：
   ```bash
   xcodebuild -workspace Actrium.xcworkspace -scheme Actrium build
   ```

4. **运行应用**：从 Xcode 运行或打开 `.app` 文件

## 注意事项

- Bundle 文件必须添加到 Xcode 项目的 Resources 中
- 修改源码后需要重新运行 bundle 脚本
- Debug 构建时，8081 端口的 "Connection refused" 错误是正常的（来自 Hermes Inspector）
- Release bundle 使用 `--dev false` 和 `--minify true` 优化

## 文件位置

- **Bundle 输出**：`Actrium/Resources/`
- **Xcode 项目**：`Actrium.xcodeproj`
- **构建脚本**：`scripts/bundle-actrium-runtime.sh`
