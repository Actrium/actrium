#!/usr/bin/env bash
set -euo pipefail

if [[ "${ACTRIUM_CONFIRM_HISTORY_REWRITE:-}" != "rewrite" ]]; then
  echo "Set ACTRIUM_CONFIRM_HISTORY_REWRITE=rewrite to confirm the history rewrite." >&2
  exit 2
fi

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

if [[ -n "$(git status --porcelain)" ]]; then
  echo "The working tree must be clean before rewriting history." >&2
  exit 2
fi

if [[ -n "$(git stash list)" ]]; then
  echo "Move or drop safety stashes after creating an external backup." >&2
  exit 2
fi

git filter-repo --force \
  --filename-callback '
primary = b"as" + b"kc"
lower = filename.lower()
drop_suffixes = (
    b"." + primary,
    b"/cli/" + primary + b".ts",
    b"/scripts/generate-" + primary + b"-signature.js",
    b"/src/cli/" + primary + b"-footer.js",
)
if any(lower.endswith(suffix) for suffix in drop_suffixes):
    return None
filename = filename.replace(b"AS" + b"KC", b"ACTR")
filename = filename.replace(b"As" + b"kc", b"Actr")
filename = filename.replace(primary, b"actr")
regex = __import__("re")
peer = b"as" + b"ks"
filename = regex.sub(rb"(?<![A-Za-z0-9_])" + peer + rb"(?![A-Za-z0-9_])", b"requests", filename)
filename = regex.sub(rb"(?<![A-Za-z0-9_])" + peer.title() + rb"(?![A-Za-z0-9_])", b"Requests", filename)
filename = regex.sub(rb"(?<![A-Za-z0-9_])" + peer.upper() + rb"(?![A-Za-z0-9_])", b"REQUESTS", filename)
return filename
' \
  --blob-callback '
if b"\0" in blob.data[:8192]:
    return
primary = b"as" + b"kc"
data = blob.data
data = data.replace(b"AS" + b"KC", b"ACTR")
data = data.replace(b"As" + b"kc", b"Actr")
data = data.replace(primary, b"actr")
regex = __import__("re")
peer = b"as" + b"ks"
data = regex.sub(rb"(?<![A-Za-z0-9_])" + peer + rb"(?![A-Za-z0-9_])", b"requests", data)
data = regex.sub(rb"(?<![A-Za-z0-9_])" + peer.title() + rb"(?![A-Za-z0-9_])", b"Requests", data)
data = regex.sub(rb"(?<![A-Za-z0-9_])" + peer.upper() + rb"(?![A-Za-z0-9_])", b"REQUESTS", data)
blob.data = data
' \
  --message-callback '
primary = b"as" + b"kc"
message = message.replace(b"AS" + b"KC", b"ACTR")
message = message.replace(b"As" + b"kc", b"Actr")
message = message.replace(primary, b"actr")
regex = __import__("re")
peer = b"as" + b"ks"
message = regex.sub(rb"(?<![A-Za-z0-9_])" + peer + rb"(?![A-Za-z0-9_])", b"requests", message)
message = regex.sub(rb"(?<![A-Za-z0-9_])" + peer.title() + rb"(?![A-Za-z0-9_])", b"Requests", message)
message = regex.sub(rb"(?<![A-Za-z0-9_])" + peer.upper() + rb"(?![A-Za-z0-9_])", b"REQUESTS", message)
return message
'

echo "History rewrite completed. Audit every retained ref before pushing."
