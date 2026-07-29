# Windows

Windows platform apps live under this directory.

The Actro browser shell entrypoint is:

```text
apps/windows/actro/
```

The current Windows client is a WebView2 browser shell with a native
tab strip, duplicate-tab command, desktop menu, navigation chrome, page find,
permission prompts,
external-protocol handoff, new-window-to-tab handling, download handling, page
zoom, address bar search fallback, navigation failure error pages,
native home-page document generation, recoverable WebView2 process-failure
reloads, lightweight content blocking, local browser history with a generated
`actro://history` page, in-page filtering, and in-page removal, local bookmarks
with a generated `actro://bookmarks` page, in-page filtering, and in-page
removal, persisted search engine selection for address-bar search fallback,
persisted startup session-restore preference, open-downloads-folder and
recent-download shell integration, page address copy/open actions, window
geometry restore, provider sync settings, and a host/page message bridge for
core status, local page management, and browser tab/navigation actions.
Keep this directory as a platform landing page only;
app-owned source, build files, and Windows-specific README content should live
under `apps/windows/actro/`.
