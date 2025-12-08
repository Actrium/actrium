# 贡献指南

感谢你对 Rill 的兴趣！本指南将帮助你开始贡献。

[English Version](./CONTRIBUTING.md)

## 开发环境要求

- **Node.js**: >= 18.0.0
- **npm**: >= 9.0.0

## 快速开始

```bash
# 1. 克隆仓库
git clone https://github.com/kookyleo/rill.git
cd rill

# 2. 安装依赖
npm install

# 3. 构建项目
npm run build

# 4. 运行类型检查
npm run typecheck

# 5. 运行测试
npm test
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
# 类型检查
npm run typecheck

# 构建
npm run build

# 运行测试
npm test

# 测试 CLI
node dist/cli/index.js build examples/test.tsx
```

### 4. 提交更改

```bash
git add .
git commit -m "feat: 添加新功能"
# 或
git commit -m "fix: 修复某个问题"
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

## CI/CD

项目使用 GitHub Actions 进行持续集成：

- ✅ 类型检查
- ✅ 构建测试
- ✅ CLI 功能测试
- ✅ 多 Node.js 版本测试 (18.x, 20.x, 22.x)

所有 PR 必须通过 CI 检查才能合并。

## 代码规范

### TypeScript

- 启用严格模式 (`strict: true`)
- 禁止隐式 any (`noImplicitAny: true`)
- 使用索引签名访问属性 (`obj['key']` 而不是 `obj.key`)

### 命名规范

- 文件名: `kebab-case.ts`
- 类型/接口: `PascalCase`
- 函数/变量: `camelCase`
- 常量: `UPPER_SNAKE_CASE`

### 错误处理

始终提供有用的错误消息：

```typescript
if (!fs.existsSync(filePath)) {
  console.error(`❌ 错误: 找不到文件: ${filePath}`);
  console.error(`\n💡 提示: 请检查路径是否正确`);
  throw new Error(`File not found: ${filePath}`);
}
```

## 项目结构

```
rill/
├── src/
│   ├── sdk/          # SDK - 供插件开发使用
│   ├── runtime/      # Runtime - 供宿主应用使用
│   ├── cli/          # CLI - 构建工具
│   ├── reconciler/   # Reconciler - React 渲染器
│   └── types/        # 类型定义
├── dist/             # 构建输出
├── examples/         # 示例项目
└── .github/
    └── workflows/    # CI 配置
```

## 性能优化

CLI 构建会显示性能指标：

```bash
✅ Build successful!
   File: dist/bundle.js
   Size: 2.95 KB
   Time: 10ms
```

保持构建时间和 bundle 大小在合理范围内。

## 需要帮助？

- 📖 [项目文档](../README.md)
- 🐛 [报告问题](https://github.com/kookyleo/rill/issues)
- 💬 [讨论区](https://github.com/kookyleo/rill/discussions)
