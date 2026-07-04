# Contributing to askit

感谢你对 askit 的关注！我们欢迎所有形式的贡献。

## 开发环境设置

### 前置要求

- [Bun](https://bun.sh/) >= 1.0.0
- Git

### 初始化

```bash
# Clone 仓库
git clone https://github.com/Actrium/askit.git
cd askit

# 安装依赖
bun install

# 运行类型检查
bun run typecheck

# 运行测试
bun test

# 运行测试（watch 模式）
bun run test:watch
```

## 项目结构

```
askit/
├── src/
│   ├── api/           # API 实现（EventEmitter, Toast, Haptic）
│   │   ├── *.host.ts    # Host (React Native) 实现
│   │   └── *.guest.ts   # Guest (QuickJS) 实现
│   ├── ui/            # UI 组件
│   │   ├── */
│   │   │   ├── *.host.tsx  # Native 组件
│   │   │   └── *.guest.tsx # DSL 生成器
│   ├── core/          # 核心桥接和注册（仅 Host）
│   ├── types/         # TypeScript 类型定义
│   ├── index.host.ts  # Host 入口
│   └── index.guest.ts # Guest 入口
├── docs/              # 文档
└── .github/           # CI/CD 配置
```

## 开发工作流

### 1. 创建分支

```bash
git checkout -b feature/your-feature-name
# 或
git checkout -b fix/your-bug-fix
```

### 2. 进行开发

- 遵循现有的代码风格
- 为新功能编写测试
- 更新相关文档

### 3. 运行检查

```bash
# 类型检查
bun run typecheck

# Lint
bun run lint

# 格式化
bun run fmt

# 测试
bun test
```

### 4. 提交代码

我们使用 [Conventional Commits](https://www.conventionalcommits.org/) 规范：

```bash
# 功能
git commit -m "feat: add new Toast animation option"

# 修复
git commit -m "fix: resolve EventEmitter message routing issue"

# 文档
git commit -m "docs: update integration guide"

# 重构
git commit -m "refactor: simplify Bridge adapter logic"

# 测试
git commit -m "test: add tests for Haptic API"
```

**提交类型**：
- `feat`: 新功能
- `fix`: Bug 修复
- `docs`: 文档更新
- `style`: 代码格式（不影响功能）
- `refactor`: 重构
- `test`: 测试
- `chore`: 构建/工具链更新

### 5. 推送并创建 Pull Request

```bash
git push origin feature/your-feature-name
```

然后在 GitHub 上创建 Pull Request。

## Pull Request 指南

### PR 标题

使用与 commit 相同的格式：

```
feat: add batch message processing
fix: prevent race condition in EventEmitter events
docs: improve API reference for Toast
```

### PR 描述模板

```markdown
## 描述
简要说明这个 PR 做了什么

## 动机
为什么需要这个改动？

## 变更内容
- 改动 1
- 改动 2

## 测试
如何测试这些改动？

## 相关 Issue
Closes #123
```

### PR 检查清单

- [ ] 代码遵循项目风格
- [ ] 通过所有测试
- [ ] 添加了必要的测试
- [ ] 更新了相关文档
- [ ] 没有引入破坏性变更（或在 PR 中明确说明）

## 代码规范

### TypeScript

- 使用 TypeScript strict 模式
- 为所有公共 API 提供类型
- 避免使用 `any`
- 使用描述性的变量和函数名

### 命名约定

```typescript
// 文件名：kebab-case
// bus-api.ts, toast-handler.ts

// 类型/接口：PascalCase
interface ToastOptions { }
type EventPayload = { };

// 函数/变量：camelCase
const handleMessage = () => { };
let currentState = { };

// 常量：UPPER_SNAKE_CASE
const MAX_RETRY_COUNT = 3;
```

### 错误处理

```typescript
// ✅ 好的
export async function show(message: string): Promise<void> {
  if (!message) {
    throw new Error('Toast message cannot be empty');
  }
  // ...
}

// ❌ 不好
export function show(message: string) {
  // 没有参数验证
  // 没有错误处理
}
```

### 注释

```typescript
// ✅ 为复杂逻辑添加注释
// Parse event format: "askit:module:method"
const parts = event.slice(6).split(':');

// ❌ 不要为显而易见的代码添加注释
// Set message to empty string
const message = '';
```

## 测试

### 编写测试

```typescript
// good-test.test.ts
// 使用 bun test：全局提供 describe/it/expect，无需从 vitest 导入
import { GuestEventEmitter } from './EventEmitter.guest';

describe('EventEmitter', () => {
  let emitter: GuestEventEmitter;

  beforeEach(() => {
    emitter = new GuestEventEmitter();
  });

  it('should emit and receive events', () => {
    const handler = vi.fn();
    emitter.on('test:event', handler);
    emitter.emit('test:event', { data: 'test' });

    expect(handler).toHaveBeenCalledWith({ data: 'test' });
  });

  it('should handle errors gracefully', () => {
    // Test error scenarios
  });
});
```

### 运行特定测试

```bash
# 运行单个文件
bun test src/api/EventEmitter.guest.test.ts

# Watch 模式
bun run test:watch
```

## 文档

### 更新文档

如果你的改动影响到 API 或行为：

1. 更新 `docs/en/` 和 `docs/zh-hans/` 中的相关文档
2. 更新 README.md（如果需要）
3. 在代码中添加 JSDoc 注释

### JSDoc 示例

```typescript
/**
 * Display a toast notification
 *
 * @param message - The message to display
 * @param options - Toast options
 * @param options.duration - Display duration ('short' | 'long' | number in ms)
 * @param options.position - Toast position
 * @returns Promise that resolves when toast is shown
 * @throws {ToastError} If message is empty
 * @throws {TimeoutError} If host doesn't respond within 5s
 *
 * @example
 * ```typescript
 * await Toast.show('Hello!', {
 *   duration: 'long',
 *   position: 'top'
 * });
 * ```
 */
export async function show(
  message: string,
  options?: ToastOptions
): Promise<void> {
  // ...
}
```

## 发布流程

（仅维护者）

1. 更新版本号：`bun version`
2. 创建 Git tag：`git tag v0.x.0`
3. 推送 tag：`git push origin v0.x.0`
4. GitHub 自动发布

## 获取帮助

- 💬 创建 [GitHub Discussion](https://github.com/Actrium/askit/discussions)
- 🐛 报告 Bug：[GitHub Issues](https://github.com/Actrium/askit/issues)
- 📧 联系维护者：87705880+kookyleo@users.noreply.github.com

## 行为准则

请阅读我们的 [Code of Conduct](./CODE_OF_CONDUCT.md)，我们期待所有贡献者都能遵守。

---

再次感谢你的贡献！🎉
