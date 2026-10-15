#!/bin/sh
# Check checksum enforcement and cache recovery without making network requests.
set -eu
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$script_dir/runner-install-lib.sh"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
printf 'trusted artifact\n' > "$work/fixture"
digest=$(sha256sum "$work/fixture" | cut -d ' ' -f 1)
RUNNER_DOWNLOAD_CACHE="$work/cache"
export RUNNER_DOWNLOAD_CACHE

curl() {
    printf 'download\n' >> "$work/calls"
    while [ "$#" -gt 0 ]; do
        case "$1" in
            -o) cp "$work/fixture" "$2"; return ;;
            *) shift ;;
        esac
    done
    return 1
}

if download_artifact https://example.invalid/tool '' "$work/output"; then
    echo 'Missing checksum was accepted' >&2; exit 1
fi
if download_artifact https://example.invalid/tool "${digest%?}z" "$work/output"; then
    echo 'Malformed checksum was accepted' >&2; exit 1
fi
test ! -f "$work/calls"

download_artifact https://example.invalid/tool "$digest" "$work/output"
cmp "$work/fixture" "$work/output"
download_artifact https://example.invalid/tool "$digest" "$work/output"
test "$(wc -l < "$work/calls")" -eq 1

cache_key=$(printf '%s' https://example.invalid/tool | sha256sum | cut -d ' ' -f 1)
printf 'corrupted cache\n' > "$RUNNER_DOWNLOAD_CACHE/$cache_key"
download_artifact https://example.invalid/tool "$digest" "$work/output"
cmp "$work/fixture" "$work/output"
test "$(wc -l < "$work/calls")" -eq 2

# A conditional caller must still receive failure; it disables shell errexit.
wrong_digest=$(printf 'untrusted artifact\n' | sha256sum | cut -d ' ' -f 1)
if download_artifact https://example.invalid/wrong "$wrong_digest" "$work/output"; then
    echo 'Checksum mismatch was accepted' >&2; exit 1
fi
wrong_key=$(printf '%s' https://example.invalid/wrong | sha256sum | cut -d ' ' -f 1)
test ! -f "$RUNNER_DOWNLOAD_CACHE/$wrong_key"

curl() { return 22; }
if download_artifact https://example.invalid/unavailable "$digest" "$work/output"; then
    echo 'Download failure was accepted' >&2; exit 1
fi
echo 'Pinned downloads, cache reuse, corruption recovery and rejection checks passed'
