#!/usr/bin/env bash
# Build + run the QuickJS CDP relay e2e (test_qjs_cdp_e2e.cpp): the QuickJS
# engine debugger driven through the full CDP relay stack. Portable C/C++, builds
# locally with clang; the vendored engine is built with -DKEEL_QJS_DEBUG.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
VENDOR="$HERE/../vendor"
QSRC="$HERE/../src"
CORE="$HERE/../../core/src"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

CC="${CC:-clang}"
CXX="${CXX:-clang++}"
CFLAGS="-DKEEL_QJS_DEBUG -D_GNU_SOURCE -O0 -g -funsigned-char -I $VENDOR"
# The native/core devtools relay is gated behind KEEL_WIP_CDP_DEVTOOLS (off by
# default in production); the engine debug hook behind KEEL_QJS_DEBUG. Both are
# needed to exercise the full QuickJS CDP stack, matching the pod's quickjs opt-in.
CXXFLAGS="-std=c++17 -DKEEL_QJS_DEBUG -DKEEL_WIP_CDP_DEVTOOLS=1 -g -I $VENDOR -I $QSRC -I $CORE"

for c in quickjs libregexp libunicode cutils; do
  $CC $CFLAGS -c "$VENDOR/$c.c" -o "$TMP/$c.o"
done

for cpp in "$QSRC/QuickJSDebugCore.cpp" "$QSRC/QuickJSEngineDebugger.cpp" \
           "$CORE/devtools/DebuggerAdapter.cpp" \
           "$CORE/devtools/AdapterDebugTarget.cpp" \
           "$CORE/devtools/CDPServer.cpp" \
           "$CORE/devtools/cdp_wire.cpp" \
           "$HERE/test_qjs_cdp_e2e.cpp"; do
  $CXX $CXXFLAGS -c "$cpp" -o "$TMP/$(basename "$cpp").o"
done

$CXX "$TMP"/*.o -lpthread -lm -ldl -o "$TMP/qjs_cdp_e2e"
"$TMP/qjs_cdp_e2e"
