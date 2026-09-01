# 贡献指南

感谢你对 Keel 的兴趣！

[English Version](./CONTRIBUTING.md)

## 开发环境要求

- **Bun**: >= 1.0.0
- **Node.js**: >= 18.0.0

## 快速开始

```bash
# 克隆仓库
git clone https://github.com/Actrium/actrium.git
cd actrium/packages/keel

# 安装依赖
bun install

# 运行测试
bun run test:ci

# 类型检查
bun run check:types

# 代码检查
bun run lint

# 文档站（http://localhost:5173/）
cd ../../keel && npm ci && npm run docs:dev
```

## 开发流程

### 1. 创建分支

```bash
git checkout -b feature/your-feature-name
# 或
git checkout -b fix/your-bug-fix
```

### 2. 进行更改

- 遵循现有代码风格
- 添加必要的类型定义
- 更新相关文档

### 3. 测试更改

```bash
# CI 测试集
bun run test:ci

# 仅单元测试
bun run test:unit

# 类型检查
bun run check:types

# 代码检查
bun run lint
```

### 4. 提交更改

```bash
git add .
git commit -m "feat: add new feature"
```

提交信息格式：
- `feat:` 新功能
- `fix:` 修复问题
- `docs:` 文档更新
- `refactor:` 代码重构
- `test:` 添加测试
- `chore:` 构建/工具更改

### 5. 推送并创建 PR

```bash
git push origin feature/your-feature-name
```

然后在 GitHub 上创建 Pull Request。

## 项目结构

```
packages/keel/                 # TypeScript 与原生实现
├── src/                       # Host、Guest SDK、Bridge、CLI 与 DevTools
├── native/                    # JSC、Hermes、QuickJS 与共享原生代码
└── tests/                     # 包内集成与 E2E 测试
crates/keel-native-guest/      # 可复用的 Rust Guest SDK
contracts/sandbox/             # Host/Guest 权威线协议
examples/keel/                 # 平台示例与 Rust Guest workload
keel/docs/                     # Keel 文档站
```

## 代码规范

### TypeScript

- 启用严格模式
- 禁止隐式 any
- 使用 Biome 进行格式化和检查

### 命名规范

- 文件名: `kebab-case.ts`
- 类型/接口: `PascalCase`
- 函数/变量: `camelCase`
- 常量: `UPPER_SNAKE_CASE`

## 需要帮助？

- [项目文档](./docs/README.zh.md)
- [报告问题](https://github.com/Actrium/actrium/issues)
