#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
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
-g !packages/supramark/**
-g !third_party/**
-g !internal/**
-g !**/*.lock
-g !**/yarn.lock
-g !**/bun.lock
-g !**/pnpm-lock.yaml
-g !scripts/check-migration-hygiene.sh
"

old_product_matches=$(
  # shellcheck disable=SC2086
  rg -n --hidden '(AskAway|askaway|Ask Away|GoAskAway|CanvasApp|CanvasReactNativeHost|Canvas-Bridging-Header|UnifiedCanvasView|canvasMode|ReactNativeTabModel|kotlin-canvas|canvas-kotlin|DataStreamApp|data_stream_app|broswer)' \
    README.md docs apps packages crates platform scripts \
    $common_globs 2>/dev/null || true
)

if [ -n "$old_product_matches" ]; then
  print_matches_failure "old product or repository names remain in first-party files" "$old_product_matches"
else
  print_ok "old product/repository names are absent from first-party files"
fi

old_runtime_matches=$(
  # shellcheck disable=SC2086
  rg -n --hidden '(RCTNApps|rctnApps|rctnapp|bundle-rctnapp|main\.rctnapp)' \
    README.md docs apps packages crates platform scripts examples package.json \
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
    README.md docs apps packages crates platform scripts examples package.json \
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
    apps/macos/actro/Actro \
    apps/macos/actro/ActroTests \
    apps/macos/actro/AGENTS.md \
    crates/actro-core/src \
    $common_globs 2>/dev/null || true
)

if [ -n "$old_macos_app_scheme_matches" ]; then
  print_matches_failure "old macOS app URL scheme remains in primary code or examples" "$old_macos_app_scheme_matches"
else
  print_ok "macOS app URLs use actro:// as the primary scheme"
fi

macos_bundle_identity_failures=""
if ! plutil -extract CFBundleDisplayName raw -o - apps/macos/actro/Actro/Info.plist 2>/dev/null | grep -qx 'Actro'; then
  macos_bundle_identity_failures="${macos_bundle_identity_failures}apps/macos/actro/Actro/Info.plist CFBundleDisplayName must be Actro
"
fi

if ! plutil -extract CFBundleName raw -o - apps/macos/actro/Actro/Info.plist 2>/dev/null | grep -qx 'Actro'; then
  macos_bundle_identity_failures="${macos_bundle_identity_failures}apps/macos/actro/Actro/Info.plist CFBundleName must be Actro
"
fi

if ! plutil -p apps/macos/actro/Actro/Info.plist 2>/dev/null | rg -q '"actro"'; then
  macos_bundle_identity_failures="${macos_bundle_identity_failures}apps/macos/actro/Actro/Info.plist must register the actro URL scheme
"
fi

if [ -n "$macos_bundle_identity_failures" ]; then
  print_matches_failure "macOS bundle identity is not Actro" "$macos_bundle_identity_failures"
else
  print_ok "macOS bundle identity is Actro"
fi

ios_bundle_identity_failures=""
if ! plutil -extract CFBundleDisplayName raw -o - apps/ios/actro/Actro/Info.plist 2>/dev/null | grep -qx 'Actro'; then
  ios_bundle_identity_failures="${ios_bundle_identity_failures}apps/ios/actro/Actro/Info.plist CFBundleDisplayName must be Actro
"
fi

if ! plutil -extract CFBundleName raw -o - apps/ios/actro/Actro/Info.plist 2>/dev/null | grep -qx 'Actro'; then
  ios_bundle_identity_failures="${ios_bundle_identity_failures}apps/ios/actro/Actro/Info.plist CFBundleName must be Actro
"
fi

if ! plutil -p apps/ios/actro/Actro/Info.plist 2>/dev/null | rg -q '"actro"'; then
  ios_bundle_identity_failures="${ios_bundle_identity_failures}apps/ios/actro/Actro/Info.plist must register the actro URL scheme
"
fi

for host in go.actro.chat go-actro.betaenv.com go-actro.develenv.com; do
  for file in \
    apps/ios/actro/Actro/Models/ActroUniversalLink.swift \
    apps/ios/actro/Actro/Actro.entitlements
  do
    if ! rg -q -F "$host" "$file" 2>/dev/null; then
      ios_bundle_identity_failures="${ios_bundle_identity_failures}${file} must include ${host} for Actro links
"
    fi
  done
done

if [ -n "$ios_bundle_identity_failures" ]; then
  print_matches_failure "iOS app identity or Actro links are incomplete" "$ios_bundle_identity_failures"
else
  print_ok "iOS app identity and Actro links are complete"
fi

android_app_json_failures=""
if ! rg -q '"name": "actro"' apps/android/actro/app.json 2>/dev/null; then
  android_app_json_failures="${android_app_json_failures}apps/android/actro/app.json must register the React Native component as actro
"
fi

if ! rg -q '"displayName": "Actro"' apps/android/actro/app.json 2>/dev/null; then
  android_app_json_failures="${android_app_json_failures}apps/android/actro/app.json must display the app as Actro
"
fi

if [ -n "$android_app_json_failures" ]; then
  print_matches_failure "Android React Native app identity is not Actro" "$android_app_json_failures"
else
  print_ok "Android React Native app identity is Actro"
fi

old_path_matches=$(
  # shellcheck disable=SC2086
  rg --files --hidden \
    -g '!**/.git/**' \
    $common_globs \
    | rg '(^|/)(Canvas|canvas|AskAway|askaway|kotlin-canvas|RCTNApps|rctnapp|ReactNativeTabModel|datastreamapp)(/|$|[._-])' || true
)

if [ -n "$old_path_matches" ]; then
  print_matches_failure "old app/client names remain in first-party paths" "$old_path_matches"
else
  print_ok "old app/client names are absent from first-party paths"
fi

unexpected_app_roots=$(
  git ls-files -z apps \
    | tr '\0' '\n' \
    | rg -v '^(apps/(android|ios|macos|windows)(/|$))' || true
)

if [ -n "$unexpected_app_roots" ]; then
  print_matches_failure "tracked files exist under unexpected app roots" "$unexpected_app_roots"
else
  print_ok "tracked app files are limited to android/ios/macos/windows roots"
fi

missing_app_roots=""
for app_root in \
  apps/macos/actro \
  apps/ios/actro \
  apps/android/actro \
  apps/windows/actro
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
  readme="apps/${platform}/README.md"
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

platform_readme_mismatches=""
for platform in macos ios android windows
do
  readme="apps/${platform}/README.md"
  if [ -f "$readme" ] && ! rg -q "apps/${platform}/actro/" "$readme"; then
    platform_readme_mismatches="${platform_readme_mismatches}${readme}
"
  fi
done

if [ -n "$platform_readme_mismatches" ]; then
  print_matches_failure "platform landing READMEs do not point at their Actro app roots" "$platform_readme_mismatches"
else
  print_ok "platform landing READMEs point at Actro app roots"
fi

imported_placeholder_matches=$(
  rg -n --hidden 'placeholder' apps/macos/README.md apps/ios/README.md apps/android/README.md 2>/dev/null || true
)

if [ -n "$imported_placeholder_matches" ]; then
  print_matches_failure "imported platform landing READMEs still describe clients as placeholders" "$imported_placeholder_matches"
else
  print_ok "imported platform landing READMEs are not placeholders"
fi

runtime_link_failures=""
if [ ! -d "packages/actro-runtime" ]; then
  runtime_link_failures="${runtime_link_failures}packages/actro-runtime is missing
"
fi

if [ ! -L "apps/ios/actro/ActroRuntime" ]; then
  runtime_link_failures="${runtime_link_failures}apps/ios/actro/ActroRuntime is not a symlink
"
elif [ "$(readlink "apps/ios/actro/ActroRuntime")" != "../../../packages/actro-runtime" ]; then
  runtime_link_failures="${runtime_link_failures}apps/ios/actro/ActroRuntime -> $(readlink "apps/ios/actro/ActroRuntime")
"
fi

if [ ! -L "apps/android/actro/actro-runtime" ]; then
  runtime_link_failures="${runtime_link_failures}apps/android/actro/actro-runtime is not a symlink
"
elif [ "$(readlink "apps/android/actro/actro-runtime")" != "../../../packages/actro-runtime" ]; then
  runtime_link_failures="${runtime_link_failures}apps/android/actro/actro-runtime -> $(readlink "apps/android/actro/actro-runtime")
"
fi

if [ -n "$runtime_link_failures" ]; then
  print_matches_failure "mobile runtime symlinks are not pointed at packages/actro-runtime" "$runtime_link_failures"
else
  print_ok "mobile runtime symlinks point at packages/actro-runtime"
fi

android_datastream_failures=""
if [ -e "apps/android/actro/android/app/src/main/java/io/actr/datastreamapp" ]; then
  android_datastream_failures="${android_datastream_failures}apps/android/actro/android/app/src/main/java/io/actr/datastreamapp should not exist; use io/actrium/actro/datastream
"
fi

if [ ! -f "apps/android/actro/android/app/src/main/java/io/actrium/actro/datastream/UnifiedWorkload.kt" ]; then
  android_datastream_failures="${android_datastream_failures}apps/android/actro/android/app/src/main/java/io/actrium/actro/datastream/UnifiedWorkload.kt is missing
"
fi

if [ ! -f "apps/android/actro/android/app/src/main/java/io/actrium/actro/datastream/generated/unified_actor.kt" ]; then
  android_datastream_failures="${android_datastream_failures}apps/android/actro/android/app/src/main/java/io/actrium/actro/datastream/generated/unified_actor.kt is missing
"
fi

if [ -n "$android_datastream_failures" ]; then
  print_matches_failure "Android datastream package layout is not under Actro" "$android_datastream_failures"
else
  print_ok "Android datastream package layout is under Actro"
fi

macos_layout_failures=""
if [ ! -d "platform/apple/packages/ActroIPC" ]; then
  macos_layout_failures="${macos_layout_failures}platform/apple/packages/ActroIPC is missing
"
fi

if [ ! -d "packages/actro-runtime" ]; then
  macos_layout_failures="${macos_layout_failures}packages/actro-runtime is missing
"
fi

if [ -e "apps/macos/actro/RCTNApps" ] || [ -L "apps/macos/actro/RCTNApps" ]; then
  macos_layout_failures="${macos_layout_failures}apps/macos/actro/RCTNApps should not exist; use packages/actro-runtime directly
"
fi

if [ -e "apps/macos/actro/.generated/rctnapp-assets" ]; then
  macos_layout_failures="${macos_layout_failures}apps/macos/actro/.generated/rctnapp-assets should not exist; use .generated/actro-runtime-assets
"
fi

if [ -e "apps/macos/actro/Packages/ActroIPC" ] || [ -L "apps/macos/actro/Packages/ActroIPC" ]; then
  macos_layout_failures="${macos_layout_failures}apps/macos/actro/Packages/ActroIPC should not exist; use platform/apple/packages/ActroIPC directly
"
fi

if [ -n "$macos_layout_failures" ]; then
  print_matches_failure "macOS compatibility layout is not pointed at monorepo locations" "$macos_layout_failures"
else
  print_ok "macOS compatibility layout points at monorepo locations"
fi

rill_demo_failures=""
for demo_root in \
  packages/rill/examples/ios-demo \
  packages/rill/examples/windows-demo
do
  if [ ! -f "${demo_root}/ReactNativeApp/index.tsx" ]; then
    rill_demo_failures="${rill_demo_failures}${demo_root}/ReactNativeApp/index.tsx is missing
"
  fi
  if [ -f "${demo_root}/package.json" ] && ! rg -q '"main": "ReactNativeApp/index\.tsx"' "${demo_root}/package.json"; then
    rill_demo_failures="${rill_demo_failures}${demo_root}/package.json does not point at ReactNativeApp/index.tsx
"
  fi
done

if [ -n "$rill_demo_failures" ]; then
  print_matches_failure "Rill demo runtime roots are not using ReactNativeApp" "$rill_demo_failures"
else
  print_ok "Rill demo runtime roots use ReactNativeApp"
fi

missing_reserved_package_readmes=""
for package_dir in \
  packages/runtime-host \
  packages/system-apps \
  packages/app-sdk \
  packages/browser-model
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
  apps/macos/askit \
  apps/macos/rill \
  apps/rustdesk-actr
do
  if [ -e "$temp_worktree" ] || [ -L "$temp_worktree" ]; then
    temporary_worktree_matches="${temporary_worktree_matches}${temp_worktree}
"
  fi
done

if [ -n "$temporary_worktree_matches" ]; then
  print_matches_failure "temporary migration worktrees must not live under apps/" "$temporary_worktree_matches"
else
  print_ok "temporary migration worktrees are absent from apps/"
fi

if [ "$failures" -ne 0 ]; then
  echo
  echo "migration hygiene failed with $failures issue(s)"
  exit 1
fi

echo
echo "migration hygiene passed"
