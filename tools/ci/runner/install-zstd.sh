#!/bin/sh
# Build the pinned CLI without re-resolving the image's apt dependencies.
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$script_dir/versions/native.env"
. "$script_dir/runner-install-lib.sh"
version="$ZSTD_VERSION"
sha256="$ZSTD_SHA256"
if zstd --version 2>/dev/null | grep -Fq "v${version},"; then
    for alias in zstdmt unzstd zstdcat; do
        ln -sf "$(command -v zstd)" "/usr/local/bin/$alias"
    done
    echo "Reusing zstd $version"
    exit 0
fi
build_dir=$(mktemp -d)
trap 'rm -rf "$build_dir"' EXIT

url="https://github.com/facebook/zstd/releases/download/v${version}/zstd-${version}.tar.gz"
if [ -n "${GITHUB_PROXY:-}" ]; then
    url="${GITHUB_PROXY%/}/${url}"
fi
download_artifact "$url" "$sha256" "$build_dir/source.tar.gz"
mkdir "$build_dir/source"
tar -xzf "$build_dir/source.tar.gz" --strip-components=1 -C "$build_dir/source"
build_jobs=${ZSTD_BUILD_JOBS:-$(getconf _NPROCESSORS_ONLN)}
case "$build_jobs" in ''|0|*[!0-9]*) echo 'ZSTD_BUILD_JOBS must be positive' >&2; exit 1 ;; esac
printf 'Building zstd %s with %s jobs\n' "$version" "$build_jobs"
make -C "$build_dir/source/programs" -j"$build_jobs" zstd
install -m 0755 "$build_dir/source/programs/zstd" /usr/local/bin/zstd
# Actions Cache invokes the threaded compressor and decompressor by alias.
for alias in zstdmt unzstd zstdcat; do
    ln -sf zstd "/usr/local/bin/$alias"
done
zstd --version | grep -Fq "v${version},"
zstdmt --version | grep -Fq "v${version},"
unzstd --version | grep -Fq "v${version},"
