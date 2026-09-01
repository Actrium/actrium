# Actro ActrApp Manifest 规范

版本：1.0.0
最后更新：2025-12-07

## 概述

Actro ActrApp 使用 `manifest.json` 文件来描述应用的元数据和布局配置。此规范参考了 Chrome Extension Manifest V3 的设计理念，采用声明式配置方式。

## 基本结构

```json
{
  "name": "应用名称",
  "version": "版本号",
  "description": "应用描述",
  "author": "作者信息",
  "layout": {
    "leftPanel": "menu.js",
    "rightPanel": "ext.js",
    "rightPanelDefaultVisible": false
  }
}
```

## 字段说明

### 必填字段

#### `name`
- **类型**: `string`
- **必填**: 是
- **说明**: 应用的唯一标识名称，建议使用 PascalCase 命名规范
- **限制**:
  - 长度：1-100 个字符
  - 格式：字母、数字、下划线，必须以字母开头
- **示例**: `"HelloWorld"`, `"IssueManager"`

#### `version`
- **类型**: `string`
- **必填**: 是
- **说明**: 应用版本号，遵循语义化版本规范 (Semantic Versioning)
- **格式**: `MAJOR.MINOR.PATCH`
- **示例**: `"1.0.0"`, `"2.3.1"`

#### `description`
- **类型**: `string`
- **必填**: 是
- **说明**: 应用的简短描述，说明应用的主要功能和用途
- **限制**:
  - 长度：10-500 个字符
  - 建议一句话说明应用的核心功能
- **示例**:
  - `"一个简单的 HelloWorld 应用示例，包含交互按钮"`
  - `"项目 Issue 管理应用，支持多项目切换和 Issue 跟踪"`

### 可选字段

#### `author`
- **类型**: `string`
- **必填**: 否
- **说明**: 应用作者或团队名称
- **示例**: `"Actro Team"`, `"张三"`

#### `layout`
- **类型**: `object`
- **必填**: 否（但推荐配置）
- **说明**: 应用的三栏布局配置对象
- **子字段**: 见下方"布局配置"章节

## 布局配置 (Layout)

Actro ActrApp 采用固定-flex-固定的三栏布局架构：

```
┌────────────┬──────────────────┬────────────┐
│  左侧菜单  │    中间对话窗口  │  右侧扩展  │
│  240px     │    flex: 1       │  400px     │
│  固定宽度  │    自适应        │  固定宽度  │
└────────────┴──────────────────┴────────────┘
```

### `layout.leftPanel`
- **类型**: `string`
- **必填**: 否
- **说明**: 左侧菜单面板的脚本文件路径（相对于应用根目录）
- **默认值**: 无（不显示左侧面板）
- **文件要求**:
  - 必须是 `.js` 文件
  - 必须导出符合 Rill Plugin 规范的 React 组件
  - 组件将在 240px 宽度的固定容器中渲染
- **示例**: `"menu.js"`, `"panels/left.js"`

### `layout.rightPanel`
- **类型**: `string`
- **必填**: 否
- **说明**: 右侧扩展面板的脚本文件路径（相对于应用根目录）
- **默认值**: 无（不显示右侧面板）
- **文件要求**:
  - 必须是 `.js` 文件
  - 必须导出符合 Rill Plugin 规范的 React 组件
  - 组件将在 400px 宽度的固定容器中渲染
- **示例**: `"ext.js"`, `"panels/right.js"`

### `layout.rightPanelDefaultVisible`
- **类型**: `boolean`
- **必填**: 否
- **说明**: 右侧扩展面板的初始可见性状态
- **默认值**: `false`（默认隐藏）
- **行为**:
  - `true`: 应用启动时右侧面板默认显示
  - `false`: 应用启动时右侧面板默认隐藏
  - 用户可以通过中间对话窗口的切换按钮动态控制显示/隐藏
- **示例**: `true`, `false`

## 完整示例

### 示例 1: HelloWorld 应用（右侧面板默认显示）

```json
{
  "name": "HelloWorld",
  "version": "1.0.0",
  "description": "一个简单的 HelloWorld 应用示例，包含交互按钮",
  "author": "Actro Team",
  "layout": {
    "leftPanel": "menu.js",
    "rightPanel": "ext.js",
    "rightPanelDefaultVisible": true
  }
}
```

**效果**: 应用启动时三栏全部显示，用户可以看到完整的界面布局。

### 示例 2: IssueManager 应用（右侧面板默认隐藏）

```json
{
  "name": "IssueManager",
  "version": "1.0.0",
  "description": "项目 Issue 管理应用，支持多项目切换和 Issue 跟踪",
  "author": "Actro Team",
  "layout": {
    "leftPanel": "menu.js",
    "rightPanel": "ext.js",
    "rightPanelDefaultVisible": false
  }
}
```

**效果**: 应用启动时仅显示左侧菜单和中间对话窗口，右侧面板隐藏，用户可以点击按钮显示。

### 示例 3: 最小配置（仅对话窗口）

```json
{
  "name": "SimpleChat",
  "version": "1.0.0",
  "description": "极简对话应用，无侧边栏"
}
```

**效果**: 应用启动时仅显示中间对话窗口，无左右侧面板。

## 校验规则

Actro 宿主应用在加载 `.actr` 包时会进行以下校验：

1. **文件存在性**: `manifest.json` 文件必须存在于应用根目录
2. **JSON 格式**: 必须是合法的 JSON 格式，不允许注释或尾随逗号
3. **必填字段**: `name`, `version`, `description` 必须存在且非空
4. **版本格式**: `version` 必须符合 `x.y.z` 格式（x, y, z 为非负整数）
5. **脚本文件**: `layout.leftPanel` 和 `layout.rightPanel` 指定的文件必须存在

## 与 Chrome Extension Manifest 的对比

| 特性 | Chrome Extension | Actro ActrApp |
|------|------------------|----------------|
| **配置文件名** | `manifest.json` | `manifest.json` |
| **版本管理** | `manifest_version`, `version` | `version` |
| **应用标识** | `name`, `short_name` | `name` |
| **描述信息** | `description`, `author` | `description`, `author` |
| **布局配置** | `browser_action`, `page_action` | `layout` (左中右三栏) |
| **权限系统** | `permissions`, `host_permissions` | 暂不支持 (未来规划) |
| **后台服务** | `background`, `service_worker` | 暂不支持 (未来规划) |

**设计理念借鉴**:
- ✅ **声明式配置**: 使用 JSON 声明应用结构，而非命令式代码
- ✅ **最小必填**: 仅 3 个必填字段，简化开发者负担
- ✅ **语义化版本**: 采用标准的版本号规范
- ✅ **扩展性**: 未来可添加权限、生命周期钩子等高级特性

## 未来规划

以下特性正在规划中，暂未实现：

### 1. 权限系统
```json
{
  "permissions": [
    "storage",
    "network",
    "filesystem"
  ]
}
```

### 2. 生命周期钩子
```json
{
  "hooks": {
    "onInstall": "scripts/install.js",
    "onUpdate": "scripts/update.js"
  }
}
```

### 3. 内容安全策略 (CSP)
```json
{
  "content_security_policy": {
    "extension_pages": "script-src 'self'; object-src 'self'"
  }
}
```

### 4. 国际化支持
```json
{
  "default_locale": "zh_CN",
  "locales": {
    "en": {
      "description": "A simple HelloWorld application"
    },
    "zh_CN": {
      "description": "一个简单的 HelloWorld 应用"
    }
  }
}
```

## 迁移指南

### 从 Chrome Extension 迁移

如果您有 Chrome Extension 开发经验，可以按照以下映射关系迁移：

| Chrome Extension | Actro ActrApp | 说明 |
|------------------|----------------|------|
| `manifest_version` | 无需配置 | Actro 自动识别版本 |
| `name` | `name` | 直接复制 |
| `version` | `version` | 直接复制 |
| `description` | `description` | 直接复制 |
| `author` | `author` | 直接复制 |
| `browser_action.default_popup` | `layout.leftPanel` / `layout.rightPanel` | 根据布局需求选择 |
| `background.service_worker` | 暂不支持 | 未来版本将支持 |
| `permissions` | 暂不支持 | 未来版本将支持 |

## 常见问题

### Q1: 如果不配置 `layout` 字段会怎样？
**A**: 应用将仅显示中间对话窗口，左右侧面板不会渲染。

### Q2: 可以动态修改 `rightPanelDefaultVisible` 吗？
**A**: 不可以。此字段仅影响应用启动时的初始状态，运行时的显示/隐藏由用户通过切换按钮控制。

### Q3: `leftPanel` 和 `rightPanel` 可以指向同一个文件吗？
**A**: 可以，但不推荐。两个面板会分别独立实例化组件，可能导致状态不同步。

### Q4: 支持嵌套路径吗（如 `panels/menu/index.js`）？
**A**: 支持。路径相对于应用根目录，可以使用 `/` 分隔符指定子目录。

### Q5: 脚本文件可以使用 TypeScript 吗？
**A**: 不可以。目前仅支持编译后的 JavaScript 文件。建议使用构建工具（如 Babel, Webpack）预编译 TypeScript。

## 参考资料

- [Chrome Extension Manifest V3 文档](https://developer.chrome.com/docs/extensions/mv3/manifest/)
- [语义化版本规范 (Semantic Versioning)](https://semver.org/)
- [Actro ActrApp 插件开发指南](./plugin-development.md)
- [Rill Plugin 规范](./rill-plugin-spec.md)

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|----------|
| 1.0.0 | 2025-12-07 | 初始版本，定义基础字段和布局配置 |
