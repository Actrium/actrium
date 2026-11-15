# Legacy commit mapping

GitHub rejected the original commit IDs because their author email is private (GH007). The private `kookyleo/actrium` repository contains a 39-commit replay from the same base snapshot with a noreply email. Patch order, messages, and final tree are preserved; commit hashes differ.

| Original | Private replay | Subject |
| --- | --- | --- |
| `5d48aa04` | [`f2709f63`](https://github.com/kookyleo/actrium/commit/f2709f63) | fix(macos): align browser identity with WebKit |
| `5829f467` | [`ca21a4b4`](https://github.com/kookyleo/actrium/commit/ca21a4b4) | fix(macos): deduplicate WebView navigation lifecycle |
| `12a774eb` | [`6f5ff6bb`](https://github.com/kookyleo/actrium/commit/6f5ff6bb) | feat(apple): define canonical main window hierarchy |
| `cf13d83f` | [`d32401f8`](https://github.com/kookyleo/actrium/commit/d32401f8) | fix(macos): harden browser infrastructure safety |
| `6bea3b6d` | [`73b5596c`](https://github.com/kookyleo/actrium/commit/73b5596c) | feat(macos): add native built-in pages and appearance |
| `d7dd5b93` | [`17e07f7b`](https://github.com/kookyleo/actrium/commit/17e07f7b) | fix(macos): preserve secure WebKit download retries |
| `2f57d8bb` | [`30663d41`](https://github.com/kookyleo/actrium/commit/30663d41) | fix(macos): avoid renderer reloads for viewport changes |
| `c71c3abf` | [`3e200f4e`](https://github.com/kookyleo/actrium/commit/3e200f4e) | feat(macos): add compact tab toolbar interactions |
| `3453d6eb` | [`b5f3c938`](https://github.com/kookyleo/actrium/commit/b5f3c938) | fix(macos): harden downloads and media permissions |
| `e10b12e1` | [`40d930ad`](https://github.com/kookyleo/actrium/commit/40d930ad) | fix(macos): block unsafe web navigation schemes |
| `5e73ff31` | [`4b83b5ae`](https://github.com/kookyleo/actrium/commit/4b83b5ae) | fix(macos): sanitize persisted browser metadata |
| `86230d66` | [`71b217a8`](https://github.com/kookyleo/actrium/commit/71b217a8) | fix(macos): honor appearance in sidebar hover |
| `761933a3` | [`b34b3444`](https://github.com/kookyleo/actrium/commit/b34b3444) | fix(macos): normalize stored credential origins |
| `63df36bc` | [`b408602d`](https://github.com/kookyleo/actrium/commit/b408602d) | fix(macos): bound and serialize webpage dialogs |
| `815e44fb` | [`70460205`](https://github.com/kookyleo/actrium/commit/70460205) | chore(branding): add cross-platform application icons |
| `a24274b8` | [`6a61f6be`](https://github.com/kookyleo/actrium/commit/6a61f6be) | build(native): support platform-selective Supramark staging |
| `7347f25e` | [`57ab0177`](https://github.com/kookyleo/actrium/commit/57ab0177) | fix(macos): harden WebKit navigation lifecycle |
| `12edcf27` | [`d2a0b0a7`](https://github.com/kookyleo/actrium/commit/d2a0b0a7) | fix(macos): smooth tab overview transitions |
| `55f172c5` | [`26b70aba`](https://github.com/kookyleo/actrium/commit/26b70aba) | feat(macos): adapt overflowing tab strip |
| `cb417c92` | [`80c8843d`](https://github.com/kookyleo/actrium/commit/80c8843d) | fix: enable browser passkey support |
| `36532940` | [`4c040e3a`](https://github.com/kookyleo/actrium/commit/4c040e3a) | fix(macos): render overflowing tabs as accordion stack |
| `03e89dba` | [`83f3a07d`](https://github.com/kookyleo/actrium/commit/83f3a07d) | fix(macos): remove cmd-t main-thread stall |
| `a9a2178a` | [`a19d6059`](https://github.com/kookyleo/actrium/commit/a19d6059) | fix(macos): keep crowded address placeholder on one line |
| `f844a7d7` | [`3b05ee4f`](https://github.com/kookyleo/actrium/commit/3b05ee4f) | fix(macos): reveal new-tab focus ring after layout |
| `94a09234` | [`6abd736f`](https://github.com/kookyleo/actrium/commit/6abd736f) | fix(macos): stage new-tab address focus |
| `cdb5568f` | [`e54ece0b`](https://github.com/kookyleo/actrium/commit/e54ece0b) | fix(macos): refine accordion tab stacking |
| `641a05e7` | [`559b0378`](https://github.com/kookyleo/actrium/commit/559b0378) | fix(macos): strengthen address focus outline |
| `5bdc518f` | [`c63f41ea`](https://github.com/kookyleo/actrium/commit/c63f41ea) | fix(macos): dismiss focus when tab is obscured |
| `4d298d31` | [`ce9c5086`](https://github.com/kookyleo/actrium/commit/ce9c5086) | fix(macos): delay accordion tab translucency |
| `be382597` | [`b69f699b`](https://github.com/kookyleo/actrium/commit/b69f699b) | fix(macos): occlude early accordion overlap |
| `793ae8df` | [`cc3e91b7`](https://github.com/kookyleo/actrium/commit/cc3e91b7) | fix(macos): localize accordion translucency |
| `673f7af3` | [`a2dc8815`](https://github.com/kookyleo/actrium/commit/a2dc8815) | fix(macos): separate touching accordion tabs |
| `28129456` | [`315303d5`](https://github.com/kookyleo/actrium/commit/315303d5) | fix(macos): refine tab seams and focus ring |
| `95a4ccbf` | [`31ef622c`](https://github.com/kookyleo/actrium/commit/31ef622c) | fix(macos): expose live chrome between tabs |
| `ce17ad06` | [`6e19ac55`](https://github.com/kookyleo/actrium/commit/6e19ac55) | fix(macos): restore full Safari-style focus ring |
| `b9a511ca` | [`45201810`](https://github.com/kookyleo/actrium/commit/45201810) | fix(macos): localize accordion boundary cap |
| `a4e267f3` | [`a4fecc47`](https://github.com/kookyleo/actrium/commit/a4fecc47) | fix(macos): wire passkey entitlements into app target |
| `1b7f8f2f` | [`58c529c7`](https://github.com/kookyleo/actrium/commit/58c529c7) | chore(macos): use github actrium bundle namespace |
| `cd0ef8e4` | [`9ce216ed`](https://github.com/kookyleo/actrium/commit/9ce216ed) | fix(macos): refine accordion edge contours |
