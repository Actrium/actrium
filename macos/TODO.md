# Actro Browser Feature Checklist

## Status Overview

| # | Feature | Priority | Status | Detail |
|---|---------|----------|--------|--------|
| 1 | Page Loading | P0 | DONE | URLRequest loading |
| 2 | Navigation Controls | P0 | **GAP** | Missing stop() |
| 3 | Loading Progress Bar | P0 | **TODO** | No estimatedProgress |
| 4 | Title Sync | P0 | DONE | WKNavigationDelegate |
| 5 | Smart Address Bar | P1 | **GAP** | No search engine fallback |
| 6 | Multi-tab | P1 | DONE | Full implementation |
| 7 | History | P1 | DONE | SQLite backed |
| 8 | Bookmarks | P1 | DONE | Full w/ tags |
| 9 | Download Management | P2 | DONE | WKDownload + URLSession |
| 10 | Settings | P2 | **GAP** | Missing cache/search/UA |
| 11 | Permission Requests | P1 | **GAP** | No media/location prompts |
| 12 | Ad Blocking | P2 | **TODO** | No WKContentRuleList |

## Work Packages

### WP-A: Navigation & Progress (P0) → [Todo-A.md](./Todo-A.md)
- A1. Add stopLoading() + toolbar stop button
- A2. estimatedProgress → Safari-style thin progress bar

### WP-B: Smart Address Bar (P1) → [Todo-B.md](./Todo-B.md)
- B1. Search engine fallback (non-URL → Google search)
- B2. Configurable default search engine in Settings

### WP-C: Permission Requests (P1) → [Todo-C.md](./Todo-C.md)
- C1. Camera/Microphone permission dialog
- C2. Geolocation permission dialog
- C3. Permission persistence per domain

### WP-D: Settings Enhancements (P2) → [Todo-D.md](./Todo-D.md)
- D1. Cache/cookies clearing UI
- D2. User-Agent switching UI
- D3. General settings tab

### WP-E: Ad Blocking (P2) → [Todo-E.md](./Todo-E.md)
- E1. WKContentRuleList with bundled rules
- E2. Settings toggle + user-configurable rules
