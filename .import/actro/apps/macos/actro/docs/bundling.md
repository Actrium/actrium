# React Native Bundle 构建指南

Actro 项目包含两个独立的 React Native bundle，分别用于不同的 XPC Service：

| Bundle | 入口文件 | 输出文件 | 用途 |
|--------|----------|----------|------|
| System Apps | `index-system.tsx` | `main.rctnapp.jsbundle` | RCTViewService0 专用（系统应用） |
| Actr Host | `index-actr.tsx` | `actr.jsbundle` | RCTViewService1-N（插件应用） |

## 快速使用

```bash
# 构建所有 bundle（Release 模式）
./scripts/bundle-rctnapp.sh

# 仅构建 System Apps bundle
./scripts/bundle-rctnapp.sh --target system

# 仅构建 Actr Host bundle
./scripts/bundle-rctnapp.sh --target actr

# 构建 Development bundle（包含 source map）
./scripts/bundle-rctnapp.sh --dev

# 查看帮助
./scripts/bundle-rctnapp.sh --help
```

## Bundle 说明

### System Apps (`main.rctnapp.jsbundle`)

**用途**：由 RCTViewService0 加载，渲染系统内置页面。

**包含的应用**：
- `StartPageApp` - 新标签页（`about:blank`）
- `HistoryApp` - 历史记录（`about:history`）
- `BookmarksApp` - 书签管理（`about:bookmarks`）
- `DebugApp` - 调试页面（`about:debug`）

**入口文件**：`RCTNApps/index-system.tsx`

### Actr Host (`actr.jsbundle`)

**用途**：由 RCTViewService1-N 加载，渲染 `.actr` 插件应用。

**包含的功能**：
- `ActrApp` - 插件宿主环境
- `rill` / `askit` 引擎支持
- Guest sandbox 运行时

**入口文件**：`RCTNApps/index-actr.tsx`

## 源码结构

```
RCTNApps/
├── index-system.tsx      # System Apps 入口
├── index-actr.tsx        # Actr Host 入口
├── index.ts              # Legacy 统一入口（已废弃）
└── src/
    ├── system-apps/      # 系统应用源码
    │   ├── StartPageApp.tsx
    │   ├── HistoryApp.tsx
    │   ├── BookmarksApp.tsx
    │   └── DebugApp.tsx
    ├── actr-host/        # Actr 宿主源码
    │   ├── ActrApp.tsx
    │   └── BundleView.tsx
    ├── TabRouter.tsx     # 路由器（Legacy）
    └── logger.ts         # 日志工具
```

## 开发流程

1. **修改源码**：编辑 `RCTNApps/src/` 下的文件

2. **构建 bundle**：
   ```bash
   ./scripts/bundle-rctnapp.sh
   ```

3. **构建 Xcode 项目**：
   ```bash
   xcodebuild -workspace Actro.xcworkspace -scheme Actro build
   ```

4. **运行应用**：从 Xcode 运行或打开 `.app` 文件

## 注意事项

- Bundle 文件必须添加到 Xcode 项目的 Resources 中
- 修改源码后需要重新运行 bundle 脚本
- Debug 构建时，8081 端口的 "Connection refused" 错误是正常的（来自 Hermes Inspector）
- Release bundle 使用 `--dev false` 和 `--minify true` 优化

## 文件位置

- **Bundle 输出**：`Actro/Resources/`
- **Xcode 项目**：`Actro.xcodeproj`
- **构建脚本**：`scripts/bundle-rctnapp.sh`
