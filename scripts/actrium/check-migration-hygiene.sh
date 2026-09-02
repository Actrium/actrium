#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
cd "$repo_root"

if ! command -v rg >/dev/null 2>&1; then
  echo "error: ripgrep (rg) is required for migration hygiene checks" >&2
  exit 127
fi

failures=0

record_failure() {
  failures=$((failures + 1))
}

print_matches_failure() {
  description=$1
  matches=$2

  echo
  echo "[fail] $description"
  printf '%s\n' "$matches"
  record_failure
}

print_ok() {
  echo "[ok] $1"
}

common_globs="
-g !**/node_modules/**
-g !**/build/**
-g !**/Pods/**
-g !**/.cxx/**
-g !**/*.xcframework/**
-g !third_party/**
-g !internal/**
-g !**/*.lock
-g !**/yarn.lock
-g !**/bun.lock
-g !**/pnpm-lock.yaml
-g !scripts/actrium/check-migration-hygiene.sh
"

old_product_matches=$(
  # shellcheck disable=SC2086
  rg -n --hidden '(AskAway|askaway|Ask Away|GoAskAway|ReactNativeTabModel|kotlin-canvas|canvas-kotlin|broswer)' \
    README.md docs packages crates scripts android ios macos windows \
    $common_globs 2>/dev/null || true
)

retired_browser_name=$(printf '%s%s' 'act' 'ro')
retired_browser_title=$(printf '%s%s' 'Act' 'ro')
retired_browser_upper=$(printf '%s%s' 'ACT' 'RO')
retired_browser_pattern="${retired_browser_name}|${retired_browser_title}|${retired_browser_upper}"
old_browser_name_matches=$(
  # Keep the retired spelling assembled at runtime so the guard does not
  # contain the value that it rejects.
  git grep -n \
    -e "$retired_browser_name" \
    -e "$retired_browser_title" \
    -e "$retired_browser_upper" \
    -- . 2>/dev/null || true
)

if [ -n "$old_browser_name_matches" ]; then
  print_matches_failure "retired browser name remains in first-party files" "$old_browser_name_matches"
else
  print_ok "retired browser name is absent from first-party files"
fi

old_browser_path_matches=$(
  git ls-files | rg "$retired_browser_pattern" || true
)

if [ -n "$old_browser_path_matches" ]; then
  print_matches_failure "retired browser name remains in first-party paths" "$old_browser_path_matches"
else
  print_ok "retired browser name is absent from first-party paths"
fi

if [ -n "$old_product_matches" ]; then
  print_matches_failure "old product or repository names remain in first-party files" "$old_product_matches"
else
  print_ok "old product/repository names are absent from first-party files"
fi

old_runtime_matches=$(
  # shellcheck disable=SC2086
  rg -n --hidden '(RCTNApps|rctnApps|rctnapp|bundle-rctnapp|main\.rctnapp)' \
    README.md docs packages crates scripts examples android ios macos windows package.json \
    $common_globs 2>/dev/null || true
)

if [ -n "$old_runtime_matches" ]; then
  print_matches_failure "old macOS runtime names remain in first-party files" "$old_runtime_matches"
else
  print_ok "old macOS runtime names are absent from first-party files"
fi

old_android_datastream_matches=$(
  # shellcheck disable=SC2086
  rg -n --hidden 'io\.actr\.datastreamapp' \
    README.md docs packages crates scripts examples android ios macos windows package.json \
    $common_globs 2>/dev/null || true
)

if [ -n "$old_android_datastream_matches" ]; then
  print_matches_failure "old Android datastream package remains in first-party files" "$old_android_datastream_matches"
else
  print_ok "old Android datastream package is absent from first-party files"
fi

old_macos_app_scheme_matches=$(
  # shellcheck disable=SC2086
  rg -n --hidden '(askw|askwURL)' \
    macos/Actrium \
    macos/ActriumTests \
    macos/AGENTS.md \
    crates/actrium-core/src \
    $common_globs 2>/dev/null || true
)

if [ -n "$old_macos_app_scheme_matches" ]; then
  print_matches_failure "old macOS app URL scheme remains in primary code or examples" "$old_macos_app_scheme_matches"
else
  print_ok "macOS app URLs use actr:// as the primary scheme"
fi

macos_bundle_identity_failures=""
if ! rg -U -q '<key>CFBundleDisplayName</key>[[:space:]]*<string>Actrium</string>' macos/Actrium/Info.plist; then
  macos_bundle_identity_failures="${macos_bundle_identity_failures}macos/Actrium/Info.plist CFBundleDisplayName must be Actrium
"
fi

if ! rg -U -q '<key>CFBundleName</key>[[:space:]]*<string>Actrium</string>' macos/Actrium/Info.plist; then
  macos_bundle_identity_failures="${macos_bundle_identity_failures}macos/Actrium/Info.plist CFBundleName must be Actrium
"
fi

if ! rg -U -q '<key>CFBundleURLSchemes</key>[[:space:]]*<array>[[:space:]]*<string>actr</string>' macos/Actrium/Info.plist; then
  macos_bundle_identity_failures="${macos_bundle_identity_failures}macos/Actrium/Info.plist must register the actr URL scheme
"
fi

if [ -n "$macos_bundle_identity_failures" ]; then
  print_matches_failure "macOS bundle identity is not Actrium" "$macos_bundle_identity_failures"
else
  print_ok "macOS bundle identity is Actrium"
fi

ios_bundle_identity_failures=""
if ! rg -U -q '<key>CFBundleDisplayName</key>[[:space:]]*<string>Actrium</string>' ios/Actrium/Info.plist; then
  ios_bundle_identity_failures="${ios_bundle_identity_failures}ios/Actrium/Info.plist CFBundleDisplayName must be Actrium
"
fi

if ! rg -U -q '<key>CFBundleName</key>[[:space:]]*<string>Actrium</string>' ios/Actrium/Info.plist; then
  ios_bundle_identity_failures="${ios_bundle_identity_failures}ios/Actrium/Info.plist CFBundleName must be Actrium
"
fi

if ! rg -U -q '<key>CFBundleURLSchemes</key>[[:space:]]*<array>[[:space:]]*<string>actr</string>' ios/Actrium/Info.plist; then
  ios_bundle_identity_failures="${ios_bundle_identity_failures}ios/Actrium/Info.plist must register the actr URL scheme
"
fi

for host in go.actrium.chat go-actrium.betaenv.com go-actrium.develenv.com; do
  for file in \
    ios/Actrium/Models/ActriumUniversalLink.swift \
    ios/Actrium/Actrium.entitlements
  do
    if ! rg -q -F "$host" "$file" 2>/dev/null; then
      ios_bundle_identity_failures="${ios_bundle_identity_failures}${file} must include ${host} for Actrium links
"
    fi
  done
done

if [ -n "$ios_bundle_identity_failures" ]; then
  print_matches_failure "iOS app identity or Actrium links are incomplete" "$ios_bundle_identity_failures"
else
  print_ok "iOS app identity and Actrium links are complete"
fi

android_app_json_failures=""
if ! rg -q '"name": "actrium"' android/app.json 2>/dev/null; then
  android_app_json_failures="${android_app_json_failures}android/app.json must register the React Native component as actrium
"
fi

if ! rg -q '"displayName": "Actrium"' android/app.json 2>/dev/null; then
  android_app_json_failures="${android_app_json_failures}android/app.json must display the app as Actrium
"
fi

if [ -n "$android_app_json_failures" ]; then
  print_matches_failure "Android React Native app identity is not Actrium" "$android_app_json_failures"
else
  print_ok "Android React Native app identity is Actrium"
fi

old_path_matches=$(
  # shellcheck disable=SC2086
  rg --files --hidden \
    -g '!**/.git/**' \
    $common_globs \
    | rg '(^|/)(AskAway|askaway|kotlin-canvas|RCTNApps|rctnapp|ReactNativeTabModel|datastreamapp)(/|$|[._-])' || true
)

if [ -n "$old_path_matches" ]; then
  print_matches_failure "old app/client names remain in first-party paths" "$old_path_matches"
else
  print_ok "old app/client names are absent from first-party paths"
fi

missing_app_roots=""
for app_root in \
  macos \
  ios \
  android \
  windows
do
  if [ ! -d "$app_root" ]; then
    missing_app_roots="${missing_app_roots}${app_root}
"
  fi
done

if [ -n "$missing_app_roots" ]; then
  print_matches_failure "required platform app roots are missing" "$missing_app_roots"
else
  print_ok "required platform app roots exist"
fi

missing_platform_readmes=""
for platform in macos ios android windows
do
  readme="${platform}/README.md"
  if [ ! -f "$readme" ]; then
    missing_platform_readmes="${missing_platform_readmes}${readme}
"
  fi
done

if [ -n "$missing_platform_readmes" ]; then
  print_matches_failure "platform landing READMEs are missing" "$missing_platform_readmes"
else
  print_ok "platform landing READMEs exist"
fi

imported_placeholder_matches=$(
  rg -n --hidden 'Status: Placeholder' macos/README.md ios/README.md android/README.md windows/README.md 2>/dev/null || true
)

if [ -n "$imported_placeholder_matches" ]; then
  print_matches_failure "imported platform landing READMEs still describe clients as placeholders" "$imported_placeholder_matches"
else
  print_ok "imported platform landing READMEs are not placeholders"
fi

runtime_link_failures=""
if [ ! -d "packages/actrium" ]; then
  runtime_link_failures="${runtime_link_failures}packages/actrium is missing
"
fi

if [ -n "$runtime_link_failures" ]; then
  print_matches_failure "shared Actrium runtime package is missing" "$runtime_link_failures"
else
  print_ok "mobile applications share packages/actrium"
fi

android_datastream_failures=""
if [ -e "android/android/app/src/main/java/io/actr/datastreamapp" ]; then
  android_datastream_failures="${android_datastream_failures}android/android/app/src/main/java/io/actr/datastreamapp should not exist; use io/actrium/app/datastream
"
fi

if [ ! -f "android/android/app/src/main/java/io/actrium/app/datastream/UnifiedWorkload.kt" ]; then
  android_datastream_failures="${android_datastream_failures}android/android/app/src/main/java/io/actrium/app/datastream/UnifiedWorkload.kt is missing
"
fi

if [ ! -f "android/android/app/src/main/java/io/actrium/app/datastream/generated/unified_actor.kt" ]; then
  android_datastream_failures="${android_datastream_failures}android/android/app/src/main/java/io/actrium/app/datastream/generated/unified_actor.kt is missing
"
fi

if [ -n "$android_datastream_failures" ]; then
  print_matches_failure "Android datastream package layout is not under Actrium" "$android_datastream_failures"
else
  print_ok "Android datastream package layout is under Actrium"
fi

macos_layout_failures=""
if [ ! -d "packages/actrium-platform/apple/ActriumIPC" ]; then
  macos_layout_failures="${macos_layout_failures}packages/actrium-platform/apple/ActriumIPC is missing
"
fi

if [ ! -d "packages/actrium" ]; then
  macos_layout_failures="${macos_layout_failures}packages/actrium is missing
"
fi

if [ -e "macos/RCTNApps" ] || [ -L "macos/RCTNApps" ]; then
  macos_layout_failures="${macos_layout_failures}macos/RCTNApps should not exist; use packages/actrium directly
"
fi

if [ -e "macos/.generated/rctnapp-assets" ]; then
  macos_layout_failures="${macos_layout_failures}macos/.generated/rctnapp-assets should not exist; use .generated/actrium-runtime-assets
"
fi

if [ -e "macos/Packages/ActriumIPC" ] || [ -L "macos/Packages/ActriumIPC" ]; then
  macos_layout_failures="${macos_layout_failures}macos/Packages/ActriumIPC should not exist; use packages/actrium-platform/apple/ActriumIPC directly
"
fi

if [ -n "$macos_layout_failures" ]; then
  print_matches_failure "macOS compatibility layout is not pointed at monorepo locations" "$macos_layout_failures"
else
  print_ok "macOS compatibility layout points at monorepo locations"
fi

keel_demo_failures=""
for demo_root in \
  examples/keel/ios-demo \
  examples/keel/windows-demo
do
  if [ ! -f "${demo_root}/ReactNativeApp/index.tsx" ]; then
    keel_demo_failures="${keel_demo_failures}${demo_root}/ReactNativeApp/index.tsx is missing
"
  fi
  if [ -f "${demo_root}/package.json" ] && ! rg -q '"main": "ReactNativeApp/index\.tsx"' "${demo_root}/package.json"; then
    keel_demo_failures="${keel_demo_failures}${demo_root}/package.json does not point at ReactNativeApp/index.tsx
"
  fi
done

if [ -n "$keel_demo_failures" ]; then
  print_matches_failure "Keel demo runtime roots are not using ReactNativeApp" "$keel_demo_failures"
else
  print_ok "Keel demo runtime roots use ReactNativeApp"
fi

missing_reserved_package_readmes=""
for package_dir in \
  packages/actrium-runtime-host \
  packages/actrium-system-apps \
  packages/actrium-app-sdk \
  packages/actrium-browser-model
do
  if [ ! -f "${package_dir}/README.md" ]; then
    missing_reserved_package_readmes="${missing_reserved_package_readmes}${package_dir}/README.md
"
  fi
done

if [ -n "$missing_reserved_package_readmes" ]; then
  print_matches_failure "reserved package boundaries are missing README files" "$missing_reserved_package_readmes"
else
  print_ok "reserved package boundaries have README files"
fi

if ! git check-ignore -q internal/rust-native-boundary.md 2>/dev/null; then
  print_matches_failure "internal local memory directory is not ignored" "internal/rust-native-boundary.md"
else
  print_ok "internal local memory directory is git-ignored"
fi

temporary_worktree_matches=""
for temp_worktree in \
  .import/actrium/apps \
  .import/actrium/packages
do
  if [ -e "$temp_worktree" ] || [ -L "$temp_worktree" ]; then
    temporary_worktree_matches="${temporary_worktree_matches}${temp_worktree}
"
  fi
done

if [ -n "$temporary_worktree_matches" ]; then
  print_matches_failure "temporary Actrium import trees remain" "$temporary_worktree_matches"
else
  print_ok "temporary Actrium import trees are absent"
fi

if [ "$failures" -ne 0 ]; then
  echo
  echo "migration hygiene failed with $failures issue(s)"
  exit 1
fi

echo
echo "migration hygiene passed"
