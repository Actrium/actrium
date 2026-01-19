# React Native New Architecture on macOS

## Overview

This document records the investigation into enabling New Architecture (Fabric + TurboModules + Bridgeless) for react-native-macos.

## Current Project Status

| Item | Value |
|------|-------|
| react-native-macos version | 0.79.1 |
| Architecture | **Legacy (Bridge)** |
| Hermes runtime | Enabled |
| Fabric | Disabled |

## How to Enable New Architecture

```bash
# Before pod install
export RCT_NEW_ARCH_ENABLED=1
pod install
```

## Expected Performance Benefits

| Feature | Benefit |
|---------|---------|
| **TurboModules** | Lazy loading of native modules, faster startup |
| **Fabric** | Synchronous rendering, better UI responsiveness |
| **JSI** | Direct JS↔Native calls, no JSON serialization overhead |
| **Bridgeless** | No RCTBridge creation overhead (~200-400ms saved) |

## react-native-macos New Architecture Support Status

### Timeline

- **2023 (v0.71)**: Experimental Fabric preview introduced
- **2023 (PR #1853)**: Template updated to support New Architecture
- **2025 (v0.79)**: Active Fabric fixes (accessibility, TextField, etc.)
- **2026 (v0.81.0)**: Latest release (Jan 17, 2026)

### Key Points

1. **Fabric is actively developed** - v0.79+ contains Fabric-specific fixes
2. **Not default** - Must explicitly enable via `RCT_NEW_ARCH_ENABLED=1`
3. **Community module compatibility** - Main concern, some modules need Fabric ComponentViews
4. **Microsoft's stance** - "Suitable for early adopters comfortable with work-in-progress experience"

### Known Fabric Fixes in v0.79

- `fix(fabric, a11y): implement accessibility role`
- `fix(fabric): Add typingAttributes to RCTUITextField`

## References

- [react-native-macos releases](https://github.com/microsoft/react-native-macos/releases)
- [New Architecture on RNW 0.76/0.77](https://devblogs.microsoft.com/react-native/2025-01-29-new-architecture-on-0-76-0-77/)
- [GitHub Issue #1841 - Enable New Arch](https://github.com/microsoft/react-native-macos/issues/1841)
- [PR #1853 - Template update](https://github.com/microsoft/react-native-macos/pull/1853)

## Testing Plan (TODO)

1. Create experimental branch
2. Enable New Architecture: `export RCT_NEW_ARCH_ENABLED=1 && pod install`
3. Test basic functionality
4. Measure Tab creation performance
5. Check for crashes/rendering issues
6. Evaluate stability for production use

## Decision

**Current decision**: Keep Legacy Architecture (Bridge mode)

**Reason**: Stability > Performance for now. Will revisit after experimental testing.
