# Actrium Import Record

Actrium was imported into Actrium with its useful source history preserved and
its Git identities privacy-sanitized before the merge.

## History boundary

- Source commits examined: 4,387.
- Commits retained: 2,270.
- Commits pruned because their changes were completely absent from the retained
  tip: 2,117.
- Sanitized source tip: `507c1dfb`.
- Actrium import merge: `f9cc7736`.

The retained commits keep author names. Known GitHub contributors use their
`ID+USERNAME@users.noreply.github.com` identities; the source `root` identity
remains anonymous. Tags and source branches were deliberately not imported.

## Layout mapping

```text
apps/android/actrium/        -> android/
apps/ios/actrium/            -> ios/
apps/macos/actrium/          -> macos/
apps/windows/actrium/        -> windows/
crates/actrium-core/         -> crates/actrium-core/
crates/actrium-sync-server/  -> crates/actrium-sync/
crates/actrium-vault/        -> crates/actrium-vault/
packages/actrium-runtime/    -> packages/actrium/
packages/actrium-platform/   -> packages/actrium-platform/
packages/askit/            -> packages/actrium-app-sdk/
```

Standalone repository manifests and workflows were folded into the root
workspace. Supramark was removed rather than imported; the browser currently
renders the affected chat content through its native text surface.

## Language policy

English is the default repository language. Human-readable translations use a
locale marker such as `README.zh.md`; application translations live under a
locale directory such as `locales/zh-Hans/`. Tracked paths are ASCII. The root
language-policy gate scans every tracked UTF-8 text file and allows a literal
non-English value only when an adjacent, reasoned exception documents a real
product or protocol requirement.

## Issue migration

The integrated code was pushed before issue transfer. GitHub preserved the
issue author, creation time, body, state, and `enhancement` label; the source
URL now resolves to the destination issue.

| Source | Destination | Title |
| --- | --- | --- |
| `Actrium/actrium#1` | `Actrium/actrium#179` | Add built-in proxy configuration support across Actrium |
