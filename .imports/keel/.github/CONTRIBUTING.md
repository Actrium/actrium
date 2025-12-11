# Contributing Guide

Thank you for your interest in contributing to Rill! This guide will help you get started.

[中文版本](./CONTRIBUTING.zh.md)

## Development Environment

- **Node.js**: >= 18.0.0
- **npm**: >= 9.0.0

## Quick Start

```bash
# 1. Clone the repository
git clone https://github.com/kookyleo/rill.git
cd rill

# 2. Install dependencies
npm install

# 3. Build the project
npm run build

# 4. Run type checking
npm run typecheck

# 5. Run tests
npm test
```

## Development Workflow

### 1. Create a Branch

```bash
git checkout -b feature/your-feature-name
# or
git checkout -b fix/your-bug-fix
```

### 2. Make Changes

- Follow existing code style
- Add necessary type definitions
- Update relevant documentation

### 3. Test Changes

```bash
# Type checking
npm run typecheck

# Build
npm run build

# Run tests
npm test

# Test CLI
node dist/cli/index.js build examples/test.tsx
```

### 4. Commit Changes

```bash
git add .
git commit -m "feat: add new feature"
# or
git commit -m "fix: fix some issue"
```

Commit message format:
- `feat:` New feature
- `fix:` Bug fix
- `docs:` Documentation update
- `refactor:` Code refactoring
- `test:` Add tests
- `chore:` Build/tooling changes

### 5. Push and Create PR

```bash
git push origin feature/your-feature-name
```

Then create a Pull Request on GitHub.

## CI/CD

The project uses GitHub Actions for continuous integration:

- ✅ Type checking
- ✅ Build tests
- ✅ CLI functionality tests
- ✅ Multi-Node.js version testing (18.x, 20.x, 22.x)

All PRs must pass CI checks before merging.

## Code Standards

### TypeScript

- Enable strict mode (`strict: true`)
- No implicit any (`noImplicitAny: true`)
- Use bracket notation for index signatures (`obj['key']` instead of `obj.key`)

### Naming Conventions

- File names: `kebab-case.ts`
- Types/Interfaces: `PascalCase`
- Functions/Variables: `camelCase`
- Constants: `UPPER_SNAKE_CASE`

### Error Handling

Always provide helpful error messages:

```typescript
if (!fs.existsSync(filePath)) {
  console.error(`❌ Error: File not found: ${filePath}`);
  console.error(`\n💡 Tip: Please check if the path is correct`);
  throw new Error(`File not found: ${filePath}`);
}
```

## Project Structure

```
rill/
├── src/
│   ├── sdk/          # SDK - For guest development
│   ├── runtime/      # Runtime - For host applications
│   ├── cli/          # CLI - Build tool
│   ├── reconciler/   # Reconciler - React renderer
│   └── types/        # Type definitions
├── dist/             # Build output
├── examples/         # Example projects
└── .github/
    └── workflows/    # CI configuration
```

## Performance Optimization

CLI builds display performance metrics:

```bash
✅ Build successful!
   File: dist/bundle.js
   Size: 2.95 KB
   Time: 10ms
```

Keep build times and bundle sizes reasonable.

## Need Help?

- 📖 [Documentation](../README.md)
- 🐛 [Report Issues](https://github.com/kookyleo/rill/issues)
- 💬 [Discussions](https://github.com/kookyleo/rill/discussions)
