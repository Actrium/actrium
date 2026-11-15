# Legacy client changes from August 25

The complete 39-commit comparison is recorded in [the delta review](legacy-39-delta-review.md), [the private file inventory](https://github.com/kookyleo/actrium/blob/audit/legacy-39-file-review/docs/migration/legacy-39-file-audit.md), and [the original-to-replay commit map](legacy-39-commit-map.md).

This records the selection from the 22 commits between upstream commit `755dea237` and private fork commit `03e89dba1`. It also covers the later 17 local commits previously reviewed for PR #452. Direct cherry-picks would replace newer window, runtime, identity, and branding code.

| Source commits | Decision in PR #452 |
| --- | --- |
| `c71c3abf3`, `55f172c56`, `365329404`: compact tabs, width compression, and edge stacks | Already adapted in `CompactTabStripView`, `CompactTabLayout`, and their layout checks. |
| `12edcf272`, `03e89dba1`: overview transitions and new-tab latency | Fade the snapshot layer into the live destination; avoid a global SwiftUI animation and synchronous database write when adding a blank tab; skip whole-window capture for native or empty tabs and avoid a second bitmap draw. |
| `12a774eb1`: Apple main-window structure | Give the iPad main window its own view while retaining the current phone entry point, system pages, search, universal links, and package drops. Keep the current package model. |
| `815e44fbc`: cross-platform app icons | Generate macOS, Android, and Windows resources from the current Actrium SVG. iOS already uses its newer `.icon` source. Do not restore the superseded blue-and-yellow mark. |
| `6bea3b6d1`, `86230d66a`: built-in pages, appearance, and sidebar hover | Current runtime owns built-in pages. The re-review restores semantic workspace/card/Start Page colors, glass frame presets, app appearance selection, and live color updates. The compact window has no sidebar; its hover colors use the active appearance. |
| `5d48aa041`, `5829f4677`, `cf13d83f2`, `d7dd5b934`, `2f57d8bbe`, `3453d6eb5`, `e10b12e18`, `5e73ff311`, `761933a39`, `63df36bc6`, `7347f25e3` | Skip ordinary WebKit, download, permission, dialog, and browser-data changes as requested. |
| `cb417c921`, `a24274b8e` | Skip passkey and native package staging as requested. |

To regenerate icons, install `librsvg` and ImageMagick, then run `scripts/actrium/generate-app-icons.sh`. The source is `ios/Actrium/Resources/actrium-logo.icon/Assets/color-and-text-restored.svg`.

## August 25–28 local interaction follow-up

The archived August 28 client (`cd0ef8e4`) is the interaction and window layout reference for the compact macOS toolbar. PR #452 adapts its in-place tab and group drag, group headers and collapse, tab context menu, active-tab address and navigation, staged focus, and accordion reveal to the current `BrowserViewModel`. The main window hosts content directly without the former sidebar split. The toolbar keeps the tabs, New Tab, and Tab Overview; its navigation and close controls live in the active tab. Its compact tab geometry and focus presentation cover these commits: `a9a2178a`, `f844a7d7`, `94a09234`, `cdb5568f`, `641a05e7`, `5bdc518f`, `4d298d31`, `be382597`, `793ae8df`, `673f7af3`, `28129456`, `95a4ccbf`, `ce17ad06`, `b9a511ca`, and `cd0ef8e4`.

`a4e267f3` is the passkey entitlement change and remains excluded. `1b7f8f2f` uses an obsolete bundle namespace; the current naming work is separate from the toolbar interaction port.
