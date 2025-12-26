#!/bin/bash
# Build QuickJS Sandbox for WebAssembly
#
# Prerequisites:
#   - Emscripten SDK installed (https://emscripten.org/docs/getting_started/downloads.html)
#   - Source the emsdk environment:
#     source /path/to/emsdk/emsdk_env.sh
#
# Usage:
#   ./build-wasm.sh [release|debug]

set -e

BUILD_TYPE="${1:-release}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BUILD_DIR="${SCRIPT_DIR}/build-wasm"
OUTPUT_DIR="${SCRIPT_DIR}/../../src/sandbox/wasm"

echo "🚀 Building QuickJS Sandbox for WebAssembly"
echo "   Build type: ${BUILD_TYPE}"
echo "   Output: ${OUTPUT_DIR}"
echo ""

# Check Emscripten
if ! command -v emcc &> /dev/null; then
    echo "❌ Error: Emscripten not found"
    echo "   Please install and source the emsdk environment:"
    echo "   https://emscripten.org/docs/getting_started/downloads.html"
    exit 1
fi

echo "✓ Emscripten found: $(emcc --version | head -n1)"
echo ""

# Create build directory
mkdir -p "${BUILD_DIR}"
cd "${BUILD_DIR}"

# Configure with CMake (use CMakeLists.wasm.txt)
echo "📦 Configuring CMake..."

# Backup original CMakeLists.txt and use WASM version
if [ -f ../CMakeLists.txt.bak ]; then
    rm ../CMakeLists.txt.bak
fi
cp ../CMakeLists.txt ../CMakeLists.txt.bak
cp ../CMakeLists.wasm.txt ../CMakeLists.txt

# Run emcmake with WASM config
emcmake cmake .. \
    -DBUILD_WASM=ON \
    -DCMAKE_BUILD_TYPE=$(echo "${BUILD_TYPE}" | awk '{print toupper($0)}')

# Restore original CMakeLists.txt
mv ../CMakeLists.txt.bak ../CMakeLists.txt

# Build
echo "🔨 Building..."
cmake --build . --config ${BUILD_TYPE} -j$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 4)

# Copy output
echo "📋 Copying output files..."
mkdir -p "${OUTPUT_DIR}"
cp -v quickjs_sandbox.js "${OUTPUT_DIR}/"
cp -v quickjs_sandbox.wasm "${OUTPUT_DIR}/"

# Report size
echo ""
echo "✅ Build complete!"
echo "   WASM size: $(du -h "${OUTPUT_DIR}/quickjs_sandbox.wasm" | cut -f1)"
echo "   JS size:   $(du -h "${OUTPUT_DIR}/quickjs_sandbox.js" | cut -f1)"
echo ""
echo "📍 Output location: ${OUTPUT_DIR}"
echo ""
echo "🎉 Ready to use! Import with:"
echo "   import { QuickJSNativeWASMProvider } from '@rill/core/sandbox/providers'"
