#!/usr/bin/env bash
# Complete an unsigned Xcode build using the same inside-out signing order
# for local development and distribution. Keep IDENTITY_NAME stable across
# builds so Keychain access controls can recognize the updated application.
set -euo pipefail

die() {
  echo "ERROR: $*" >&2
  exit 1
}

if [ "$#" -ne 1 ] || [ "$1" = "--help" ]; then
  echo "Usage: IDENTITY_NAME='<signing identity>' $0 <Actrium.app>"
  echo "Optional: KEYCHAIN_PATH, CODESIGN_TIMESTAMP (default: secure; none for development)."
  echo "IDENTITY_NAME=- is for signature verification fixtures only; its identity changes on rebuild."
  [ "$#" -eq 1 ] && [ "$1" = "--help" ] && exit 0
  exit 64
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
APP_PATH="$1"
APP_ENTITLEMENTS="${APP_ENTITLEMENTS:-${PROJECT_ROOT}/Actrium/Actrium.entitlements}"
RENDERER_ENTITLEMENTS="${RENDERER_ENTITLEMENTS:-${PROJECT_ROOT}/ActriumViewService/ActriumViewService.entitlements}"

[ -n "${IDENTITY_NAME:-}" ] || die "signing requires IDENTITY_NAME; use a fixed Apple Development or Developer ID identity"
[ -d "${APP_PATH}/Contents/MacOS" ] || die "not an application bundle: ${APP_PATH}"
[ -f "${APP_ENTITLEMENTS}" ] || die "entitlements file does not exist: ${APP_ENTITLEMENTS}"
[ -f "${RENDERER_ENTITLEMENTS}" ] || die "entitlements file does not exist: ${RENDERER_ENTITLEMENTS}"

CODESIGN_ARGS=(-f -o runtime -s "${IDENTITY_NAME}")
case "${CODESIGN_TIMESTAMP:-secure}" in
  secure) CODESIGN_ARGS+=(--timestamp) ;;
  none) CODESIGN_ARGS+=(--timestamp=none) ;;
  *) die "CODESIGN_TIMESTAMP must be secure or none" ;;
esac
if [ -n "${KEYCHAIN_PATH:-}" ]; then
  CODESIGN_ARGS+=(--keychain "${KEYCHAIN_PATH}")
fi

validate_executable() {
  local bundle="$1" executable
  executable="$(/usr/libexec/PlistBuddy -c 'Print:CFBundleExecutable' "${bundle}/Contents/Info.plist")"
  case "${executable}" in
    ''|*/*) die "invalid CFBundleExecutable in ${bundle}" ;;
  esac
  [ -f "${bundle}/Contents/MacOS/${executable}" ] && \
    [ -x "${bundle}/Contents/MacOS/${executable}" ] || die "missing executable in ${bundle}"
}

sign_nested_code() {
  local dir="$1"
  [ -d "${dir}" ] || return 0

  local dylib framework
  while IFS= read -r -d '' dylib; do
    codesign "${CODESIGN_ARGS[@]}" "${dylib}"
  done < <(find "${dir}" -maxdepth 1 -type f -name '*.dylib' -print0)

  while IFS= read -r -d '' framework; do
    codesign "${CODESIGN_ARGS[@]}" "${framework}"
  done < <(find "${dir}" -maxdepth 1 -type d -name '*.framework' -print0)
}

echo "==> Signing the app inside out"
HELPER_APP="${APP_PATH}/Contents/Helpers/ActriumRenderer.app"
validate_executable "${APP_PATH}"
if [ -d "${HELPER_APP}" ]; then
  validate_executable "${HELPER_APP}"
fi
if [ -d "${HELPER_APP}" ]; then
  sign_nested_code "${HELPER_APP}/Contents/Frameworks"
  codesign "${CODESIGN_ARGS[@]}" --entitlements "${RENDERER_ENTITLEMENTS}" "${HELPER_APP}"
fi

sign_nested_code "${APP_PATH}/Contents/Frameworks"
codesign "${CODESIGN_ARGS[@]}" --entitlements "${APP_ENTITLEMENTS}" "${APP_PATH}"
codesign --verify --deep --strict --verbose=2 "${APP_PATH}"
