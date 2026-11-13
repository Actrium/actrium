#!/usr/bin/env bash
#
# Build Actrium.app and package it into a distributable DMG.
#
# Flow: unsigned Release build -> optional nested manual signing
#       -> optional app notarization -> hdiutil DMG
#       -> optional DMG notarization.
#
# Usage:
#   scripts/package-dmg.sh [options]
#
# Options:
#   --unsigned          Build only; skip signing and notarization (unsigned DMG).
#   --skip-signing      Skip codesign (the build still runs).
#   --skip-notary       Skip app and DMG notarization.
#   --skip-notary-app   Skip app notarization.
#   --skip-dmg          Skip DMG creation.
#   --skip-notary-dmg   Skip DMG notarization.
#   --create-github-release  Create/update a GitHub release through gh (optional).
#   -h, --help          Show this help.
#
# Env:
#   APP_SCHEME         Default Actrium
#   CONFIGURATION      Default Release
#   DERIVED_DATA_PATH  Default ./build/DerivedData
#   DIST_DIR           Default ./dist
#   TAG                Artifact tag, default v{MARKETING_VERSION}_{CURRENT_PROJECT_VERSION}_{git short sha}
#
# Signing (required unless --skip-signing/--unsigned):
#   IDENTITY_NAME      For example "Developer ID Application: Foo (TEAMID)"
#   APP_BASE64         Optional base64-encoded .p12 (CI usage)
#   P12_FILE           Optional .p12 path (alternative to APP_BASE64)
#   P12_PASSWORD       Required when APP_BASE64/P12_FILE is used
#   KEYCHAIN_PATH      Optional keychain to import into and use
#   KEYCHAIN_PASSWORD  Required when KEYCHAIN_PATH already exists
#   KEEP_KEYCHAIN=1    Keep the temporary keychain (deleted by default)
#
# Notarization (required unless skipped):
#   NOTARY_PROFILE     Default actrium
#   USER_NAME          Apple ID
#   TEAM_ID            Apple Developer Team ID
#   ACCOUNT_PASSWORD   App-specific password
#
# GitHub release (optional):
#   GITHUB_REPOSITORY  owner/repo, inferred from origin by default
set -euo pipefail

die() {
  echo "ERROR: $*" >&2
  exit 1
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "missing command: $1"
}

is_truthy() {
  case "${1:-}" in
    1|true|TRUE|yes|YES|y|Y) return 0 ;;
    *) return 1 ;;
  esac
}

generate_password() {
  if command -v openssl >/dev/null 2>&1; then
    openssl rand -hex 16
    return 0
  fi
  if command -v uuidgen >/dev/null 2>&1; then
    uuidgen | tr -d '-'
    return 0
  fi
  LC_ALL=C tr -dc 'A-Za-z0-9' </dev/urandom | head -c 32 || true
}

usage() {
  sed -n '2,45p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

SKIP_SIGNING=0
SKIP_NOTARY_APP=0
SKIP_DMG=0
SKIP_NOTARY_DMG=0
CREATE_GITHUB_RELEASE=0

while [ "${#}" -gt 0 ]; do
  case "$1" in
    --unsigned)
      SKIP_SIGNING=1
      SKIP_NOTARY_APP=1
      SKIP_NOTARY_DMG=1
      ;;
    --skip-signing) SKIP_SIGNING=1 ;;
    --skip-notary)
      SKIP_NOTARY_APP=1
      SKIP_NOTARY_DMG=1
      ;;
    --skip-notary-app) SKIP_NOTARY_APP=1 ;;
    --skip-dmg) SKIP_DMG=1 ;;
    --skip-notary-dmg) SKIP_NOTARY_DMG=1 ;;
    --create-github-release) CREATE_GITHUB_RELEASE=1 ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      die "unknown argument: $1 (see --help for usage)"
      ;;
  esac
  shift
done

[ "$(uname -s)" = "Darwin" ] || die "this script must run on macOS."

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${PROJECT_ROOT}"

APP_SCHEME="${APP_SCHEME:-Actrium}"
CONFIGURATION="${CONFIGURATION:-Release}"
DERIVED_DATA_PATH="${DERIVED_DATA_PATH:-${PROJECT_ROOT}/build/DerivedData}"
DIST_DIR="${DIST_DIR:-${PROJECT_ROOT}/dist}"
NOTARY_PROFILE="${NOTARY_PROFILE:-actrium}"

APP_ENTITLEMENTS="${APP_ENTITLEMENTS:-${PROJECT_ROOT}/Actrium/Actrium.entitlements}"
RENDERER_ENTITLEMENTS="${RENDERER_ENTITLEMENTS:-${PROJECT_ROOT}/ActriumViewService/ActriumViewService.entitlements}"

# CocoaPods projects must build with -workspace, otherwise the Pods static
# libraries are missing from the link.
XCODE_PROJECT_FLAG=()
if [ -d "${PROJECT_ROOT}/${APP_SCHEME}.xcworkspace" ]; then
  XCODE_PROJECT_FLAG=(-workspace "${PROJECT_ROOT}/${APP_SCHEME}.xcworkspace")
elif [ -d "${PROJECT_ROOT}/${APP_SCHEME}.xcodeproj" ]; then
  XCODE_PROJECT_FLAG=(-project "${PROJECT_ROOT}/${APP_SCHEME}.xcodeproj")
else
  die "neither ${APP_SCHEME}.xcworkspace nor ${APP_SCHEME}.xcodeproj was found"
fi

need_cmd xcodebuild
need_cmd xcrun
need_cmd find
need_cmd ditto
need_cmd hdiutil

# The Xcode project is the single source of truth for the version.
if [ -z "${TAG:-}" ]; then
  echo "==> Reading the version"
  BUILD_SETTINGS="$(xcodebuild "${XCODE_PROJECT_FLAG[@]}" \
    -scheme "${APP_SCHEME}" \
    -configuration "${CONFIGURATION}" \
    -showBuildSettings 2>/dev/null || true)"

  MARKETING_VERSION="$(printf '%s\n' "${BUILD_SETTINGS}" | awk -F' = ' '/^[[:space:]]*MARKETING_VERSION = /{print $2; exit}')"
  CURRENT_PROJECT_VERSION="$(printf '%s\n' "${BUILD_SETTINGS}" | awk -F' = ' '/^[[:space:]]*CURRENT_PROJECT_VERSION = /{print $2; exit}')"
  MARKETING_VERSION="${MARKETING_VERSION:-1.0.0}"
  CURRENT_PROJECT_VERSION="${CURRENT_PROJECT_VERSION:-1}"

  if [ -n "${GITHUB_SHA:-}" ]; then
    COMMIT_HASH="${GITHUB_SHA:0:7}"
    TAG="v${MARKETING_VERSION}_${CURRENT_PROJECT_VERSION}_${COMMIT_HASH}"
  elif command -v git >/dev/null 2>&1 && git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    COMMIT_HASH="$(git rev-parse --short=7 HEAD)"
    TAG="v${MARKETING_VERSION}_${CURRENT_PROJECT_VERSION}_${COMMIT_HASH}"
  else
    TAG="v${MARKETING_VERSION}_${CURRENT_PROJECT_VERSION}"
  fi
fi

echo "==> Xcode: $(xcodebuild -version | tr '\n' ' ')"
echo "==> TAG: ${TAG}"

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/actrium-build.XXXXXX")"
KEYCHAIN_CREATED=0
IMPORTED_P12=0
KEYCHAIN_PATH="${KEYCHAIN_PATH:-}"
KEYCHAIN_PASSWORD="${KEYCHAIN_PASSWORD:-}"
P12_FILE="${P12_FILE:-}"

cleanup() {
  local exit_code=$?
  if [ "${KEYCHAIN_CREATED}" -eq 1 ] && ! is_truthy "${KEEP_KEYCHAIN:-0}"; then
    security delete-keychain "${KEYCHAIN_PATH}" >/dev/null 2>&1 || true
  fi
  rm -rf "${TMP_DIR}" >/dev/null 2>&1 || true
  return "${exit_code}"
}
trap cleanup EXIT

# The RN bundle is produced by the Xcode "Bundle Actrium Runtime" build phase
# (Release uses a production --dev false bundle), so no separate bundling step
# is needed here.
echo "==> Building the app (unsigned)"
mkdir -p "$(dirname "${DERIVED_DATA_PATH}")"
xcodebuild \
  "${XCODE_PROJECT_FLAG[@]}" \
  -scheme "${APP_SCHEME}" \
  -configuration "${CONFIGURATION}" \
  -derivedDataPath "${DERIVED_DATA_PATH}" \
  CODE_SIGNING_ALLOWED=NO \
  build

echo "==> Locating the build product"
APP_PATH="$(find "${DERIVED_DATA_PATH}/Build/Products/${CONFIGURATION}" -maxdepth 2 -type d -name "${APP_SCHEME}.app" | head -n 1 || true)"
[ -n "${APP_PATH}" ] || die "${APP_SCHEME}.app was not found under ${DERIVED_DATA_PATH}/Build/Products/${CONFIGURATION}"
echo "==> Product: ${APP_PATH}"

# Drop the DerivedData Launch Services registration so it does not shadow the
# copy installed in /Applications.
LSREGISTER="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"
if [ -x "${LSREGISTER}" ]; then
  echo "==> Clearing the Launch Services registration"
  "${LSREGISTER}" -gc 2>/dev/null || true
  "${LSREGISTER}" -u "${APP_PATH}" 2>/dev/null || true
fi

ensure_keychain_for_import() {
  [ -n "${P12_FILE}" ] || [ -n "${APP_BASE64:-}" ] || return 0

  need_cmd security
  need_cmd base64
  need_cmd tr

  if [ -z "${KEYCHAIN_PATH}" ]; then
    KEYCHAIN_PATH="${TMP_DIR}/codesign.keychain-db"
  fi

  mkdir -p "$(dirname "${KEYCHAIN_PATH}")"

  if [ -f "${KEYCHAIN_PATH}" ]; then
    [ -n "${KEYCHAIN_PASSWORD}" ] || die "KEYCHAIN_PATH already exists, provide KEYCHAIN_PASSWORD: ${KEYCHAIN_PATH}"
  fi

  if [ -z "${KEYCHAIN_PASSWORD}" ]; then
    KEYCHAIN_PASSWORD="$(generate_password)"
  fi

  if [ ! -f "${KEYCHAIN_PATH}" ]; then
    security create-keychain -p "${KEYCHAIN_PASSWORD}" "${KEYCHAIN_PATH}"
    KEYCHAIN_CREATED=1
  fi

  security unlock-keychain -p "${KEYCHAIN_PASSWORD}" "${KEYCHAIN_PATH}"
  security set-keychain-settings -lut 21600 "${KEYCHAIN_PATH}" >/dev/null 2>&1 || true

  if [ -z "${P12_FILE}" ]; then
    [ -n "${APP_BASE64:-}" ] || die "importing a certificate requires APP_BASE64 or P12_FILE"
    P12_FILE="${TMP_DIR}/cert.p12"
    printf '%s' "${APP_BASE64}" | tr -d '\n' | base64 --decode > "${P12_FILE}"
  fi

  [ -n "${P12_PASSWORD:-}" ] || die "importing a certificate requires P12_PASSWORD"
  [ -f "${P12_FILE}" ] || die "P12_FILE does not exist: ${P12_FILE}"

  echo "==> Importing the signing certificate into the keychain"
  security import "${P12_FILE}" \
    -k "${KEYCHAIN_PATH}" \
    -P "${P12_PASSWORD}" \
    -T /usr/bin/codesign \
    -T /usr/bin/security >/dev/null

  security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "${KEYCHAIN_PASSWORD}" "${KEYCHAIN_PATH}" >/dev/null 2>&1 || true
  IMPORTED_P12=1
}

# Sign the top-level .dylib and .framework entries of a directory, inside out:
# dylibs first, then frameworks.
sign_nested_code() {
  local dir="$1"

  [ -d "${dir}" ] || return 0

  while IFS= read -r -d '' dylib; do
    echo "    Signing dylib: $(basename "${dylib}")"
    codesign -f -o runtime --timestamp -s "${IDENTITY_NAME}" \
      ${CODESIGN_KEYCHAIN_ARGS[@]+"${CODESIGN_KEYCHAIN_ARGS[@]}"} "${dylib}"
  done < <(find "${dir}" -maxdepth 1 -type f -name '*.dylib' -print0)

  while IFS= read -r -d '' framework; do
    echo "    Signing framework: $(basename "${framework}")"
    codesign -f -o runtime --timestamp -s "${IDENTITY_NAME}" \
      ${CODESIGN_KEYCHAIN_ARGS[@]+"${CODESIGN_KEYCHAIN_ARGS[@]}"} "${framework}"
  done < <(find "${dir}" -maxdepth 1 -type d -name '*.framework' -print0)
}

if [ "${SKIP_SIGNING}" -eq 0 ]; then
  [ -n "${IDENTITY_NAME:-}" ] || die "signing requires IDENTITY_NAME (or use --skip-signing/--unsigned)"
  [ -f "${APP_ENTITLEMENTS}" ] || die "entitlements file does not exist: ${APP_ENTITLEMENTS}"
  [ -f "${RENDERER_ENTITLEMENTS}" ] || die "entitlements file does not exist: ${RENDERER_ENTITLEMENTS}"

  ensure_keychain_for_import

  CODESIGN_KEYCHAIN_ARGS=()
  NOTARY_KEYCHAIN_ARGS=()
  if [ "${IMPORTED_P12}" -eq 1 ]; then
    CODESIGN_KEYCHAIN_ARGS=(--keychain "${KEYCHAIN_PATH}")
    NOTARY_KEYCHAIN_ARGS=(--keychain "${KEYCHAIN_PATH}")
  elif [ -n "${KEYCHAIN_PATH}" ]; then
    NOTARY_KEYCHAIN_ARGS=(--keychain "${KEYCHAIN_PATH}")
  fi

  echo "==> Signing the app (inside out, without --deep)"

  # The embedded renderer helper (ActriumRenderer.app).
  HELPER_APP="${APP_PATH}/Contents/Helpers/ActriumRenderer.app"
  if [ -d "${HELPER_APP}" ]; then
    sign_nested_code "${HELPER_APP}/Contents/Frameworks"
    echo "    Signing helper: ActriumRenderer.app"
    codesign -f -o runtime --timestamp \
      --entitlements "${RENDERER_ENTITLEMENTS}" \
      -s "${IDENTITY_NAME}" \
      ${CODESIGN_KEYCHAIN_ARGS[@]+"${CODESIGN_KEYCHAIN_ARGS[@]}"} "${HELPER_APP}"
  fi

  # Frameworks inside the main app (Pods, SwiftPM, Hermes, etc.).
  sign_nested_code "${APP_PATH}/Contents/Frameworks"

  echo "    Signing the main app: ${APP_SCHEME}.app"
  codesign -f -o runtime --timestamp \
    --entitlements "${APP_ENTITLEMENTS}" \
    -s "${IDENTITY_NAME}" \
    ${CODESIGN_KEYCHAIN_ARGS[@]+"${CODESIGN_KEYCHAIN_ARGS[@]}"} "${APP_PATH}"

  codesign -vvv "${APP_PATH}"
else
  echo "==> Skipping signing (--skip-signing/--unsigned)"
fi

if [ "${SKIP_NOTARY_APP}" -eq 0 ] || { [ "${SKIP_DMG}" -eq 0 ] && [ "${SKIP_NOTARY_DMG}" -eq 0 ]; }; then
  [ "${SKIP_SIGNING}" -eq 0 ] || die "notarization requires signing; drop --skip-signing/--unsigned or use --skip-notary*."
  [ -n "${USER_NAME:-}" ] || die "notarization requires USER_NAME"
  [ -n "${TEAM_ID:-}" ] || die "notarization requires TEAM_ID"
  [ -n "${ACCOUNT_PASSWORD:-}" ] || die "notarization requires ACCOUNT_PASSWORD"

  echo "==> Storing the notarization credentials (profile: ${NOTARY_PROFILE})"
  xcrun notarytool store-credentials "${NOTARY_PROFILE}" \
    --apple-id "${USER_NAME}" \
    --team-id "${TEAM_ID}" \
    --password "${ACCOUNT_PASSWORD}" \
    ${NOTARY_KEYCHAIN_ARGS[@]+"${NOTARY_KEYCHAIN_ARGS[@]}"}
fi

if [ "${SKIP_NOTARY_APP}" -eq 0 ]; then
  echo "==> Notarizing the app"
  mkdir -p "${DIST_DIR}"
  APP_ZIP_PATH="${DIST_DIR}/${APP_SCHEME}_app.zip"

  echo "==> Zipping the app: ${APP_PATH} -> ${APP_ZIP_PATH}"
  ditto -c -k --keepParent "${APP_PATH}" "${APP_ZIP_PATH}"

  xcrun notarytool submit "${APP_ZIP_PATH}" \
    --keychain-profile "${NOTARY_PROFILE}" \
    ${NOTARY_KEYCHAIN_ARGS[@]+"${NOTARY_KEYCHAIN_ARGS[@]}"} \
    --wait

  xcrun stapler staple "${APP_PATH}" || true
  spctl --assess -vv --type execute "${APP_PATH}" || true
fi

DMG_PATH=""
DMG_NAME=""
if [ "${SKIP_DMG}" -eq 0 ]; then
  echo "==> Creating the DMG"
  mkdir -p "${DIST_DIR}"

  DMG_NAME="${APP_SCHEME}_${TAG}.dmg"
  DMG_PATH="${DIST_DIR}/${DMG_NAME}"

  DMG_ROOT="${TMP_DIR}/dmg_root"
  mkdir -p "${DMG_ROOT}"
  cp -R "${APP_PATH}" "${DMG_ROOT}/"
  ln -s /Applications "${DMG_ROOT}/Applications"

  hdiutil create \
    -volname "${APP_SCHEME}" \
    -srcfolder "${DMG_ROOT}" \
    -ov \
    -format UDZO \
    "${DMG_PATH}"

  [ -f "${DMG_PATH}" ] || die "DMG creation failed: ${DMG_PATH}"
  echo "==> DMG created: ${DMG_PATH}"

  if [ "${SKIP_NOTARY_DMG}" -eq 0 ]; then
    [ "${SKIP_SIGNING}" -eq 0 ] || die "DMG notarization requires signing."

    echo "==> Notarizing the DMG"
    xcrun notarytool submit "${DMG_PATH}" \
      --keychain-profile "${NOTARY_PROFILE}" \
      ${NOTARY_KEYCHAIN_ARGS[@]+"${NOTARY_KEYCHAIN_ARGS[@]}"} \
      --wait

    xcrun stapler staple "${DMG_PATH}" || true
    spctl --assess -vv --type open "${DMG_PATH}" || true
  fi
else
  echo "==> Skipping the DMG (--skip-dmg)"
fi

infer_github_repository() {
  local url
  url="$(git remote get-url origin 2>/dev/null || true)"
  [ -n "${url}" ] || return 1
  case "${url}" in
    git@github.com:*.git) echo "${url#git@github.com:}" | sed 's/\.git$//' ;;
    https://github.com/*) echo "${url#https://github.com/}" | sed 's/\.git$//' ;;
    *) return 1 ;;
  esac
}

if [ "${CREATE_GITHUB_RELEASE}" -eq 1 ]; then
  [ -n "${DMG_PATH}" ] || die "--create-github-release requires a DMG"
  need_cmd gh
  need_cmd git

  if [ -z "${GITHUB_REPOSITORY:-}" ]; then
    GITHUB_REPOSITORY="$(infer_github_repository || true)"
  fi
  [ -n "${GITHUB_REPOSITORY:-}" ] || die "GITHUB_REPOSITORY is required (for example owner/repo)"

  echo "==> Creating the GitHub release (${GITHUB_REPOSITORY}@${TAG})"
  gh release create "${TAG}" "${DMG_PATH}" \
    --repo "${GITHUB_REPOSITORY}" \
    --title "${APP_SCHEME} macOS ${TAG}" \
    --notes "Local build artifact, ${TAG}."
fi

echo "==> Done"
echo "APP_PATH=${APP_PATH}"
if [ -n "${DMG_PATH}" ]; then
  echo "DMG_PATH=${DMG_PATH}"
fi

if [ -n "${GITHUB_ENV:-}" ]; then
  {
    echo "TAG=${TAG}"
    echo "APP_PATH=${APP_PATH}"
    if [ -n "${DMG_PATH}" ]; then
      echo "DMG_PATH=${DMG_PATH}"
      echo "DMG_NAME=${DMG_NAME}"
    fi
  } >> "${GITHUB_ENV}"
fi
