# 39-commit delta review

## Rebase onto current main

The 39 historical patches were replayed in their original order onto the
current `main` in an isolated worktree. Git produced 38 commits because one
patch was already empty after conflict resolution. The compatibility commit
then removed retired app paths, passkey, Supramark staging, and
obsolete branding, while retaining the newer runtime and platform assets.
The compact macOS window is based on the August final
`RootContentViewController.swift` itself, with only current type, service,
accessibility, and product-name adaptations. This replaces the earlier
reconstructed compact strip.

This review uses the original 39 patches in order, not just the changed-path list. The exact historical tree and the 156-path inventory are in the [private comparison repository](https://github.com/kookyleo/actrium/tree/audit/legacy-39-file-review). The [commit map](legacy-39-commit-map.md) links original hashes to the replayed, privacy-safe hashes. “Excluded” means the previously agreed WebKit, passkey, or package staging scope; it does not claim the current code implements that patch.

## First 22 commits

| Original | Concrete change checked | Current resolution |
| --- | --- | --- |
| `5d48aa04` | WebKit browser identity and user agent | Excluded: ordinary WebKit behavior. |
| `5829f467` | Duplicate navigation callbacks and lifecycle | Excluded: ordinary WebKit behavior. |
| `12a774eb` | Canonical Apple main-window structure, including iPad entry | Current Apple window tree was retained; the iPad entry and phone routing were adapted earlier in PR #452. The macOS window now directly hosts tabs plus workspace in `RootContentViewController`. |
| `cf13d83f` | Browser infrastructure safety and favicon transport | Existing browser infrastructure retained. The compact header's favicon is now fetched from the page origin with a byte cap in `FaviconCache`; no third-party icon service is used by that header. |
| `6bea3b6d` | Native pages, app appearance, glass presets, semantic workspace/card colors, stable native-tab lifecycle | Appearance mode and all five glass presets are in `WindowChromeStyle` and General Settings. The frame/header material, body border, Start Page, Overview cards, transition layer, and renderer placeholders now use semantic colors. `PersistentNativeTabLayer` keeps native tabs mounted; the newer runtime keeps ownership of internal-page routing. |
| `d7dd5b93` | Secure WebKit download retry | Excluded: download implementation. |
| `2f57d8bb` | Avoid renderer reload on viewport change | Excluded: renderer lifecycle. |
| `c71c3abf` | Compact tab/group toolbar, in-place drag, address actions, window drag mask | `RootContentViewController`, `CompactTabStripView`, and `ToolbarTabInteractionCoordinator` carry the visible toolbar, group actions, tab reorder, hit regions, address controls, bookmarks/share, and active-tab favicon. New runtime APIs are kept. |
| `3453d6eb` | Download and media permissions | Excluded: download/permission implementation. |
| `e10b12e1` | Unsafe Web navigation schemes | Excluded: ordinary WebKit navigation policy. |
| `5e73ff31` | Persisted browser metadata sanitation | Excluded: browser data policy. |
| `86230d66` | Sidebar hover colors under light appearance | The compact main window has no sidebar. Semantic hover colors are used in its tab/header controls; retired sidebar rendering is not mounted. |
| `761933a3` | Stored credential origin normalization | Excluded: credential implementation. |
| `63df36bc` | Bounded webpage dialogs | Excluded: ordinary WebKit dialogs. |
| `815e44fb` | Cross-platform application icons | Current Actrium artwork and generated platform assets were retained; the superseded mark was not copied. |
| `a24274b8` | Platform-selective native package staging | Excluded: package staging. |
| `7347f25e` | WebKit navigation lifecycle hardening | Excluded: ordinary WebKit behavior. |
| `12edcf27` | Overview transition snapshot and redraw timing | Previously adapted into `ModeTransitionController` and `WorkspaceMainView`; the current workspace remains the renderer. |
| `55f172c5` | Adaptive widths and tab-strip reveal | Width calculation and reveal alignment live in `CompactTabLayout` and `CompactTabStripView`. |
| `cb417c92` | Passkey implementation | Excluded by request. |
| `36532940` | Accordion presentation for overflow | Presentation geometry, depth, and edge click disposition live in `CompactTabLayout` and `CompactTabStripView`. |
| `03e89dba` | Remove synchronous storage from new-tab path | `BrowserViewModel.addNewTab` delays persistence until navigation and requests staged address focus. |

## Later 17 commits

| Original | Exact UI delta checked | Current resolution |
| --- | --- | --- |
| `a9a2178a` | Placeholder and inactive tab title remain single-line under compression | Both toolbar address placeholder and tab labels use single-line truncation. |
| `f844a7d7` | Reveal the newly focused tab after the layout pass | `CompactTabStripView.scheduleActiveTabReveal` checks after yield and after the slide settles. |
| `94a09234` | Three stages: background, editor content, then first responder and ring | `AddressPresentationPhase` and the address field defer focus across the two original delays. Focused plate uses a semantic address surface. |
| `cdb5568f` | Incoming accordion tab sits over the edge tab; folded slices reveal before selection | Shared presentation z-index and `foldedTabButton` preserve the source click disposition. |
| `641a05e7` | Larger, softer complete focus outline | `addressFocusRing` uses the source outward 2.5-point contour and inner half-point highlight. |
| `5bdc518f` | Obscured active address ends editing and first-responder state | `CompactTabStripView` reports obscuring to the toolbar; the editor is unmounted and the interaction coordinator ends editing. |
| `4d298d31` | Delay translucent cover until late overlap | `coverHeadEdgeOpacity` in shared accordion geometry drives the cover mask. |
| `be382597` | Cover the lower tab early in overlap | A layered boundary is painted only on incoming covers, not on flat tabs. |
| `793ae8df` | Limit translucency to the directional head | The gradient changes only the first or last 30% of the cover. |
| `673f7af3` | Separate touching tabs without full outlines | Plates are inset 1.5 points horizontally; ordinary seams reveal the live header. |
| `28129456` | Color the layered seam from the selected glass tint | `WindowChromeStyle.layeredTabSeparationColor` composes the tint over the source base plate. |
| `95a4ccbf` | Expose actual frame material between flat tabs | The root uses active `NSVisualEffectView` plus a tint layer; the toolbar has no opaque background. |
| `ce17ad06` | Keep the complete focus ring outside the accordion cover mask | `addressFocusRing` is overlaid after the cover mask in `renderedTab`. |
| `b9a511ca` | Limit the overlap boundary to an 18-point fading cap | `layeredBoundaryMask` paints only the affected edge during overlap. |
| `a4e267f3` | Passkey entitlement | Excluded by request. |
| `1b7f8f2f` | Obsolete bundle namespace | Excluded; current product identifiers are handled separately. |
| `cd0ef8e4` | Edge contours for stacked accordion tabs | `stackContours` draws the source offset paths on a Canvas and applies the directional cap mask. |

## Verification

- `./macos/scripts/build-app.sh --unsigned`: passes after the re-review changes.
- `CompactTabLayoutTests`: passes; insertion slots, accordion geometry, reveal, and window drag mask are covered.
- The final build's dark window was compared on screen with the archived app. The light setting was also launched and checked for semantic frame, tab, body, and text colors. Accordion dragging and the full focus transition still require an interactive side-by-side pass.
