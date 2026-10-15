#!/bin/sh
# Build independent toolcache stages as runner, with final ownership at creation.
set -eu
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$script_dir/versions/toolcache.env"
. "$script_dir/runner-install-lib.sh"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$RUNNER_TOOL_CACHE/current"
# Java tools do not consume the standard HTTP proxy variables themselves.
if [ -n "${HTTPS_PROXY:-${HTTP_PROXY:-}}" ]; then
    proxy_options=$(python3 - <<'PY_PROXY'
import os
from urllib.parse import urlparse
proxy = urlparse(os.environ.get("HTTPS_PROXY") or os.environ["HTTP_PROXY"])
port = proxy.port or 80
print(f"-Dhttps.proxyHost={proxy.hostname} -Dhttps.proxyPort={port} "
      f"-Dhttp.proxyHost={proxy.hostname} -Dhttp.proxyPort={port}")
PY_PROXY
    )
    export JAVA_TOOL_OPTIONS="${JAVA_TOOL_OPTIONS:-} $proxy_options"
fi
export JAVA_HOME="$RUNNER_TOOL_CACHE/Java_Temurin-Hotspot_jdk/$JDK_VERSION/x64"

install_python() {
    version="$1"
    tag="$2"
    sha="$3"
    destination="$RUNNER_TOOL_CACHE/Python/$version/x64"
    mkdir -p "$destination"
    download_artifact \
        "$(github_url "https://github.com/actions/python-versions/releases/download/$tag/python-$version-linux-22.04-x64.tar.gz")" \
        "$sha" "$work/python-$version.tar.gz"
    tar -xzf "$work/python-$version.tar.gz" -C "$destination"
    ln -sf python3 "$destination/bin/python"
    ln -sf pip3 "$destination/bin/pip"
    touch "$RUNNER_TOOL_CACHE/Python/$version/x64.complete"
    "$destination/bin/python3" --version
}

case "$1" in
    python)
        install_python "$PY311" "$PY311_TAG" "$PY311_SHA256"
        install_python "$PY313" "$PY313_TAG" "$PY313_SHA256"
        ln -s "$RUNNER_TOOL_CACHE/Python/$PY311/x64" "$RUNNER_TOOL_CACHE/current/python"
        ;;
    node)
        destination="$RUNNER_TOOL_CACHE/node/$NODE_VERSION/x64"
        mkdir -p "$destination"
        download_artifact "https://nodejs.org/dist/v$NODE_VERSION/node-v$NODE_VERSION-linux-x64.tar.gz" \
            "$NODE_SHA256" "$work/node.tar.gz"
        tar -xzf "$work/node.tar.gz" --strip-components=1 -C "$destination"
        touch "$RUNNER_TOOL_CACHE/node/$NODE_VERSION/x64.complete"
        ln -s "$destination" "$RUNNER_TOOL_CACHE/current/node"
        "$destination/bin/node" --version
        ;;
    java)
        version_tag=$(printf '%s' "$JDK_VERSION" | sed 's/+/%2B/')
        version_file=$(printf '%s' "$JDK_VERSION" | tr '+' '_')
        download_artifact \
            "$(github_url "https://github.com/adoptium/temurin17-binaries/releases/download/jdk-$version_tag/OpenJDK17U-jdk_x64_linux_hotspot_$version_file.tar.gz")" \
            "$JDK_SHA256" "$work/java.tar.gz"
        mkdir -p "$JAVA_HOME"
        tar -xzf "$work/java.tar.gz" --strip-components=1 -C "$JAVA_HOME"
        touch "$RUNNER_TOOL_CACHE/Java_Temurin-Hotspot_jdk/$JDK_VERSION/x64.complete"
        ln -s "$JAVA_HOME" "$RUNNER_TOOL_CACHE/current/java"
        "$JAVA_HOME/bin/java" -version
        ;;
    android)
        download_artifact \
            "https://dl.google.com/android/repository/commandlinetools-linux-${CMDLINE_TOOLS}_latest.zip" \
            "$CMDLINE_TOOLS_SHA256" "$work/android.zip"
        mkdir -p "$ANDROID_SDK_ROOT/cmdline-tools"
        unzip -q "$work/android.zip" -d "$ANDROID_SDK_ROOT/cmdline-tools"
        mv "$ANDROID_SDK_ROOT/cmdline-tools/cmdline-tools" "$ANDROID_SDK_ROOT/cmdline-tools/latest"
        sdkmanager="$ANDROID_SDK_ROOT/cmdline-tools/latest/bin/sdkmanager"
        yes | "$sdkmanager" --licenses >/dev/null
        "$sdkmanager" --install 'platforms;android-34' 'build-tools;34.0.0' \
            'build-tools;35.0.0' 'platform-tools'
        "$sdkmanager" --list_installed
        ;;
    gradle)
        distribution_url="https://services.gradle.org/distributions/gradle-$GRADLE_VERSION-bin.zip"
        download_url="$distribution_url"
        if [ -n "${GRADLE_DIST_PROXY:-}" ]; then
            download_url="${GRADLE_DIST_PROXY%/}/gradle-$GRADLE_VERSION-bin.zip"
        fi
        key=$(python3 "$script_dir/prepare-gradle-cache.py" \
            --distribution-url "$distribution_url" --print-cache-key)
        destination="$GRADLE_USER_HOME/wrapper/dists/gradle-$GRADLE_VERSION-bin/$key"
        mkdir -p "$destination"
        download_artifact "$download_url" "$GRADLE_DIST_SHA256" "$work/gradle.zip"
        unzip -q "$work/gradle.zip" -d "$destination"
        touch "$destination/gradle-$GRADLE_VERSION-bin.zip.ok"
        "$destination/gradle-$GRADLE_VERSION/bin/gradle" --version
        mkdir -p "$work/ktlint-preseed"
        cp "$script_dir/ktlint-preseed.gradle" "$work/ktlint-preseed/build.gradle"
        "$destination/gradle-$GRADLE_VERSION/bin/gradle" \
            -p "$work/ktlint-preseed" --no-daemon \
            -PktlintVersion="$KTLINT_VERSION" -PktlintGradleVersion="$KTLINT_GRADLE_VERSION" \
            resolveKtlint
        ;;
    *) echo 'Expected python, node, java, android or gradle' >&2; exit 1 ;;
esac
