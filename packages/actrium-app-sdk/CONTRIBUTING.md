# Contributing to askit

Thank you for your right. askit Attention! We welcome all forms of contributions.

## Develop Environment Settings

### Pre-request

- [Bun](https://bun.sh/) >= 1.0.0
- Git

### Initialize

```bash
# Clone Warehouse
git clone https://github.com/Actrium/askit.git
cd askit

# Installation Dependence
bun install

# Run Type Check
bun run typecheck

# Run Test
bun test

# Run Test (watch Mode)
bun run test:watch
```

## Project structure

```
askit/
├── src/
│   ├── api/           # API Achieved (EventEmitter, Toast, Haptic)
│   │   ├── *.host.ts    # Host (React Native) Achieved
│   │   └── *.guest.ts   # Guest (QuickJS) Achieved
│   ├── ui/            # UI Component
│   │   ├── */
│   │   │   ├── *.host.tsx  # Native Component
│   │   │   └── *.guest.tsx # DSL Generator
│   ├── core/          # Core Bridges and Registration (only) Host)
│   ├── types/         # TypeScript Type definition
│   ├── index.host.ts  # Host Entry
│   └── index.guest.ts # Guest Entry
├── docs/              # Document
└── .github/           # CI/CD Configure
```

## Develop Workstream

### 1. Create Branch

```bash
git checkout -b feature/your-feature-name
# or
git checkout -b fix/your-bug-fix
```

### 2. Development

- Follow the current code style
- Prepare tests for new functionality
- Update relevant documents

### 3. Run Check

```bash
# Type check
bun run typecheck

# Lint
bun run lint

# Formatting
bun run fmt

# Test
bun test
```

### 4. Submit Code

We use [Conventional Commits](https://www.conventionalcommits.org/) Regulation:

```bash
# Functions
git commit -m "feat: add new Toast animation option"

# Rehabilitation
git commit -m "fix: resolve EventEmitter message routing issue"

# Document
git commit -m "docs: update integration guide"

# Reconstruct
git commit -m "refactor: simplify Bridge adapter logic"

# Test
git commit -m "test: add tests for Haptic API"
```

**Type of submission**:
- `feat`: New function
- `fix`: Bug Rehabilitation
- `docs`: Document Update
- `style`: Code format (without prejudice to function)
- `refactor`: Reconstruct
- `test`: Test
- `chore`: Build/Tool Chain Update

### 5. Send and Create Pull Request

```bash
git push origin feature/your-feature-name
```

And then... GitHub Up Create Pull Request.

## Pull Request Guide

### PR Title

Use and commit Same format:

```
feat: add batch message processing
fix: prevent race condition in EventEmitter events
docs: improve API reference for Toast
```

### PR Description Template

```markdown
## Description
Just a short description of this. PR What did you do?

## Motive
Why does this change need to be made?

## Change
- Changes 1
- Changes 2

## Test
How are these changes tested?

## Relevant Issue
Closes #123
```

### PR Checklist

- [ ] Code follows project style
- [ ] Pass all tests
- [ ] Add necessary tests
- [ ] Updated relevant documents
- [ ] No destructive changes introduced (or PR (as specified in the report)

## Code Code Code Coder

### TypeScript

- Use TypeScript strict Mode
- For all public API Type of supply
- Avoid Usage `any`
- Use descriptive variables and function names

### Naming Protocol

```typescript
// File name:kebab-case
// bus-api.ts, toast-handler.ts

// Type/Interface:PascalCase
interface ToastOptions { }
type EventPayload = { };

// Function/Variables:camelCase
const handleMessage = () => { };
let currentState = { };

// Constant:UPPER_SNAKE_CASE
const MAX_RETRY_COUNT = 3;
```

### Error Handling

```typescript
// ✅ Okay.
export async function show(message: string): Promise<void> {
  if (!message) {
    throw new Error('Toast message cannot be empty');
  }
  // ...
}

// ❌ Not good.
export function show(message: string) {
  // No arguments validated
  // No errors processed
}
```

### Comment

```typescript
// ✅ Add Comment for Complex Logic
// Parse event format: "askit:module:method"
const parts = event.slice(6).split(':');

// ❌ Do not add comments for the obvious code
// Set message to empty string
const message = '';
```

## Test

### Prepare tests

```typescript
// good-test.test.ts
// Use bun test: Global describe/it/expect, not from vitest Import
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

### Run Specific Tests

```bash
# Run Single File
bun test src/api/EventEmitter.guest.test.ts

# Watch Mode
bun run test:watch
```

## Document

### Update Document

If you change it, API or act:

1. Update `docs/en/` and `docs/zh-hans/` , and then the document in which the
2. Update README.md(if required)
3. Add to Code JSDoc Comment

### JSDoc Example:

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

## Publishing Process

(Container only)

1. Update version number:`bun version`
2. Create Git tag: `git tag v0.x.0`
3. Send tag: `git push origin v0.x.0`
4. GitHub Autopublishing

## Get help.

- 💬 Create [GitHub Discussion](https://github.com/Actrium/askit/discussions)
- 🐛 Report Bug: [GitHub Issues](https://github.com/Actrium/askit/issues)
- Contact the maintainers through the repository issue tracker.

## Code of conduct

Read ours. [Code of Conduct](./CODE_OF_CONDUCT.md)We expect all contributors to comply.

---

Thanks again for your contribution!🎉
