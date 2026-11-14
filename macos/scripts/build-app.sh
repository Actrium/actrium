#!/usr/bin/env bash
# Verification builds are intentionally unsigned. Interactive builds must
# finish signing before the resulting app is opened or reads the Keychain.
set -euo pipefail

UNSIGNED=0
while [ "$#" -gt 0 ]; do
  case "$1" in
    --unsigned) UNSIGNED=1 ;;
    -h|--help)
      echo "Usage: IDENTITY_NAME='<signing identity>' $0 [--unsigned]"
      echo "--unsigned: compile verification only; do not use this product for Realm sessions."
      echo "Optional: CONFIGURATION (Debug), DERIVED_DATA_PATH, KEYCHAIN_PATH."
      exit 0
      ;;
    *) echo "ERROR: unknown argument: $1" >&2; exit 64 ;;
  esac
  shift
done

if [ "${UNSIGNED}" -eq 0 ] && { [ -z "${IDENTITY_NAME:-}" ] || [ "${IDENTITY_NAME}" = "-" ]; }; then
  echo "ERROR: interactive builds require a fixed IDENTITY_NAME (Apple Development or Developer ID)." >&2
  echo "Use --unsigned only for compile verification." >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
REPO_ROOT="$(cd "${PROJECT_ROOT}/.." && pwd)"
CONFIGURATION="${CONFIGURATION:-Debug}"
DERIVED_DATA_PATH="${DERIVED_DATA_PATH:-${PROJECT_ROOT}/build/DerivedData}"

"${REPO_ROOT}/scripts/actrium/filter-xcodebuild-output.sh" xcodebuild \
  -workspace "${PROJECT_ROOT}/Actrium.xcworkspace" \
  -scheme Actrium \
  -configuration "${CONFIGURATION}" \
  -destination "platform=macOS,arch=$(uname -m)" \
  -derivedDataPath "${DERIVED_DATA_PATH}" \
  CODE_SIGNING_ALLOWED=NO \
  build

APP_PATH="${DERIVED_DATA_PATH}/Build/Products/${CONFIGURATION}/Actrium.app"
if [ "${UNSIGNED}" -eq 0 ]; then
  CODESIGN_TIMESTAMP="${CODESIGN_TIMESTAMP:-none}" "${SCRIPT_DIR}/sign-app.sh" "${APP_PATH}"
else
  echo "==> Unsigned compile verification only; use build:macos with IDENTITY_NAME for an interactive build."
fi
echo "APP_PATH=${APP_PATH}"
