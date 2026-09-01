# Actro Import Record

Actro was imported into Actrium with its useful source history preserved and
its Git identities privacy-sanitized before the merge.

## History boundary

- Source commits examined: 4,387.
- Commits retained: 2,270.
- Commits pruned because their changes were completely absent from the retained
  tip: 2,117.
- Sanitized source tip: `755dea23`.
- Actrium import merge: `40d6b1a7`.

The retained commits keep author names. Known GitHub contributors use their
`ID+USERNAME@users.noreply.github.com` identities; the source `root` identity
remains anonymous. Tags and source branches were deliberately not imported.

## Layout mapping

```text
apps/android/actro/        -> android/
apps/ios/actro/            -> ios/
apps/macos/actro/          -> macos/
apps/windows/actro/        -> windows/
crates/actro-core/         -> crates/actrium-core/
crates/actro-sync-server/  -> crates/actrium-sync/
crates/actro-vault/        -> crates/actrium-vault/
packages/actro-runtime/    -> packages/actrium/
packages/actro-platform/   -> packages/actrium-platform/
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

Issue transfer happens only after the integrated code has been pushed. Record
the source-to-destination issue mapping here after transfer so references can
be repaired without relying on issue-number equality.
