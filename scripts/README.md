# Scripts

Repeatable repository workflows. Subsystem-specific scripts should live within
that subsystem unless they coordinate the combined monorepo.

## Release notes

`CHANGELOG.md` is a fixed link to GitHub Releases. Release Prepare generates
categorized notes with `scripts/actr/generate-changelog.py` for a new release PR.
Stable versions compare against the previous stable version; prereleases use the
preceding prerelease with the same version core, falling back to a stable tag.
Maintenance releases use stable tags from their own minor line.

The release PR body is editable and becomes the final release commit body.
Refreshing an existing preparation branch preserves its PR body, so review and
update the notes for newly included changes before merging. No version notes are
committed to `CHANGELOG.md` or another version file.

After normal CI and review checks, preview the exact release commit message:

```sh
python3 scripts/actr/merge-release-pr.py <pr-number>
```

Squash-merge the Release Prepare PR into main with its title and body explicitly
preserved, guarded by the PR head SHA:

```sh
python3 scripts/actr/merge-release-pr.py <pr-number> --merge
```

The helper does not approve reviews or bypass branch protection. Keep the subject
exactly `chore(release): basic train vX.Y.Z` so the publishing workflow triggers.
The body must start with `## vX.Y.Z` and contain the reviewed release notes. When
merging through another tool (including manual maintenance releases), explicitly
supply that subject and the complete reviewed PR body as the commit body. Do not
use an empty body or GitHub's generated list of individual commits.

Publishing reads the pinned release commit body, adds the system requirements
header, and uses it as the GitHub Release body. It fails if the release subject is
invalid or the body is empty; it never silently reconstructs notes from history.
Publishing retries reuse the committed notes and preserve release assets. Changes
to the PR body after merging do not affect the committed notes.

Generate a draft from history without writing repository files:

```sh
python3 scripts/actr/generate-changelog.py --version 0.10.1 --to-ref HEAD
```

Preview the final notes from a release commit:

```sh
python3 scripts/actr/generate-changelog.py --version 0.10.1 \
  --to-ref v0.10.1 --from-release-commit --header .github/release-notes-header.md
```
