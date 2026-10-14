#!/bin/sh
# Build the pinned CLI without re-resolving the image's apt dependencies.
set -eu

version=1.5.7
sha256=eb33e51f49a15e023950cd7825ca74a4a2b43db8354825ac24fc1b7ee09e6fa3
build_dir=$(mktemp -d)
trap 'rm -rf "$build_dir"' EXIT

url="https://github.com/facebook/zstd/releases/download/v${version}/zstd-${version}.tar.gz"
if [ -n "${GITHUB_PROXY:-}" ]; then
    url="${GITHUB_PROXY%/}/${url}"
fi
curl --fail --location --retry 3 --retry-all-errors -o "$build_dir/source.tar.gz" "$url"
echo "$sha256  $build_dir/source.tar.gz" | sha256sum --check --status
mkdir "$build_dir/source"
tar -xzf "$build_dir/source.tar.gz" --strip-components=1 -C "$build_dir/source"
make -C "$build_dir/source/programs" -j2 zstd
install -m 0755 "$build_dir/source/programs/zstd" /usr/local/bin/zstd
# Actions Cache invokes the threaded compressor and decompressor by alias.
for alias in zstdmt unzstd zstdcat; do
    ln -sf zstd "/usr/local/bin/$alias"
done
zstd --version | grep -Fq "v${version},"
zstdmt --version | grep -Fq "v${version},"
unzstd --version | grep -Fq "v${version},"
