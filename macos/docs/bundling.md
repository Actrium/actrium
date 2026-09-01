# React Native Bundle Build Guide

Actro The project contains a stand-alone project React Native bundle. Current Helper It's a single. `ActroRenderer.app`Press fingerprint Segregation process, no more copying `RCTViewService*.xpc`. See also below. [Renderer-Process-Pool.md](Renderer-Process-Pool.md).

| Bundle | Entry files | Output File | Purpose |
|--------|----------|----------|------|
| System Apps | `index-system.tsx` | `system.jsbundle` | System Renderer(`system.*`) |
| Actr Host | `index-actr.tsx` | `actr.jsbundle` | Normal Renderer |

## Quick Use

```bash
# Build All bundle(Release Mode)
./scripts/bundle-actro-runtime.sh

# Build Only System Apps bundle
./scripts/bundle-actro-runtime.sh --target system

# Build Only Actr Host bundle
./scripts/bundle-actro-runtime.sh --target actr

# Build Development bundle(includes source map)
./scripts/bundle-actro-runtime.sh --dev

# View Help
./scripts/bundle-actro-runtime.sh --help
```

## Bundle Annotations

### System Apps (`system.jsbundle`)

**Purpose**: By RCTViewService0 Load, render the system to page.

**Include Applications**:
- `StartPageApp` - New Tab (`about:blank`)
- `HistoryApp` - History (Hycken)`about:history`)
- `BookmarksApp` - Bookmark Management (Bookmarks Management)`about:bookmarks`)
- `DebugApp` - Debug Page (`about:debug`)

**Entry files**: `packages/actrium/index-system.tsx`

### Actr Host (`actr.jsbundle`)

**Purpose**: By RCTViewService1-N Load, render `.actr` Plugin application.

**Include Functions**:
- `ActrApp` - Plugin Host Environment
- `keel` / `askit` Engine support
- Guest sandbox Run-time

**Entry files**: `packages/actrium/index-actr.tsx`

## Source Structure

```
packages/actrium/
├── index-system.tsx      # System Apps Entry
├── index-actr.tsx        # Actr Host Entry
├── index.ts              # Legacy Unified entrance (disused)
└── src/
    ├── system-apps/      # System application source code
    │   ├── StartPageApp.tsx
    │   ├── HistoryApp.tsx
    │   ├── BookmarksApp.tsx
    │   └── DebugApp.tsx
    ├── actr-host/        # Actr Host Source
    │   ├── ActrApp.tsx
    │   └── BundleView.tsx
    ├── TabRouter.tsx     # Router (Log)Legacy)
    └── logger.ts         # Log Tool
```

## Development Process

1. **Modify Source**: Edit `packages/actrium/src/` Next

2. **Build bundle**:
   ```bash
   ./scripts/bundle-actro-runtime.sh
   ```

3. **Build Xcode Item**:
   ```bash
   xcodebuild -workspace Actro.xcworkspace -scheme Actro build
   ```

4. **Run Application**: From Xcode Run or open `.app` Documentation

## Attention

- Bundle File must be added to Xcode Projects Resources Medium
- Rerun after changing source bundle Script
- Debug When building,8081 Port "Connection refused" The error is normal. Hermes Inspector)
- Release bundle Use `--dev false` and `--minify true` Optimization

## File Location

- **Bundle Output**: `Actro/Resources/`
- **Xcode Item**: `Actro.xcodeproj`
- **Build Script**: `scripts/bundle-actro-runtime.sh`
