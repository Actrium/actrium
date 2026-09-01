# Contributing Guide

Thank you for your interest in contributing to Keel!

[中文版本](./CONTRIBUTING.zh.md)

## Development Environment

- **Bun**: >= 1.0.0
- **Node.js**: >= 18.0.0

## Quick Start

```bash
# Clone the repository
git clone https://github.com/Actrium/actrium.git
cd actrium/packages/keel

# Install dependencies
bun install

# Run tests
bun run test:ci

# Type check
bun run check:types

# Lint
bun run lint

# Docs site (http://localhost:5173/)
cd ../../keel && npm ci && npm run docs:dev
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
# CI test suite
bun run test:ci

# Unit tests only
bun run test:unit

# Type checking
bun run check:types

# Lint
bun run lint
```

### 4. Commit Changes

```bash
git add .
git commit -m "feat: add new feature"
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

## Project Structure

```
packages/keel/                 # TypeScript and native implementation
├── src/                       # Host, guest SDK, bridge, CLI, and devtools
├── native/                    # JSC, Hermes, QuickJS, and shared native code
└── tests/                     # Package-local integration and E2E tests
crates/keel-native-guest/      # Reusable Rust guest SDK
contracts/sandbox/             # Authoritative host/guest wire contracts
examples/keel/                 # Platform demos and Rust guest workloads
keel/docs/                     # Keel documentation site
```

## Code Standards

### TypeScript

- Strict mode enabled
- No implicit any
- Use Biome for formatting and linting

### Naming Conventions

- File names: `kebab-case.ts`
- Types/Interfaces: `PascalCase`
- Functions/Variables: `camelCase`
- Constants: `UPPER_SNAKE_CASE`

## Need Help?

- [Documentation](./docs/README.md)
- [Report Issues](https://github.com/Actrium/actrium/issues)
