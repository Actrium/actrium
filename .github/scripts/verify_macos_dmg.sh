#!/usr/bin/env bash
set -euo pipefail

asset=$1
app_version=$2
build_number=$3
mount_dir=$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/actrium-mount.XXXXXX")
mounted=false
cleanup() {
  if [[ "$mounted" == true ]]; then
    hdiutil detach "$mount_dir"
  fi
  rmdir "$mount_dir"
}
trap cleanup EXIT

for attempt in 1 2 3; do
  if output=$(LC_ALL=C hdiutil attach -readonly -nobrowse -mountpoint "$mount_dir" "$asset" 2>&1); then
    printf '%s\n' "$output"
    mounted=true
    break
  fi
  printf '%s\n' "$output" >&2
  if [[ "$attempt" == 3 ]] || ! grep -Eiq 'resource (temporarily unavailable|busy)|device (or resource )?busy' <<< "$output"; then
    exit 1
  fi
  sleep "$attempt"
done

app="$mount_dir/Actrium.app/Contents"
test "$(readlink "$mount_dir/Applications")" = /Applications
test "$(lipo -archs "$app/MacOS/Actrium")" = arm64
test "$(plutil -extract CFBundleShortVersionString raw "$app/Info.plist")" = "$app_version"
test "$(plutil -extract CFBundleVersion raw "$app/Info.plist")" = "$build_number"
test -n "$(find "$app" -name system.jsbundle -size +0 -print -quit)"

if [[ "${4:-false}" == true ]]; then
  bundle="$mount_dir/Actrium.app"
  codesign --verify --strict "$asset"
  xcrun stapler validate "$asset"
  codesign --verify --deep --strict "$bundle"
  python3 - "$bundle" <<'PY'
from pathlib import Path
import plistlib
import subprocess
import sys

bundle = Path(sys.argv[1])
contents = bundle / "Contents"
profile = plistlib.loads(subprocess.check_output([
    "security", "cms", "-D", "-i", str(contents / "embedded.provisionprofile"),
]))
entitlements = plistlib.loads(subprocess.check_output([
    "codesign", "--display", "--entitlements", ":-", str(bundle),
]))
app_id = profile["Entitlements"]["com.apple.application-identifier"]
bundle_id = plistlib.loads((contents / "Info.plist").read_bytes())["CFBundleIdentifier"]
if (app_id.partition(".")[2] != bundle_id
        or entitlements.get("com.apple.application-identifier") != app_id
        or entitlements.get("com.apple.developer.team-identifier") not in profile["TeamIdentifier"]):
    sys.exit("ERROR: Mounted Actrium Host does not have its provisioned private Keychain identity")
PY
fi

shasum -a 256 "$asset" > "$asset.sha256"
shasum -a 256 -c "$asset.sha256"
