# syntax=docker/dockerfile:1.7
# vi: set ft=dockerfile:

# Actrium self-hosted GitHub Actions runner image (Linux x86_64).
#
# This image is the execution environment for actr CI jobs running on
# self-hosted Linux runners. It bundles the GitHub Actions runner agent,
# the Rust toolchain pinned to the workspace MSRV, and the system
# libraries the build needs, so a job can start without spending minutes
# on `apt-get install` / `rustup` every run.
#
# Windows runners are intentionally NOT covered here; see README.md.
#
# Build context is the directory containing this Dockerfile:
#   docker build -t actrium-runner:dev -f tools/ci/runner/Dockerfile tools/ci/runner/
#
# Under ARC (gha-runner-scale-set), the controller injects the JIT runner
# config as the ACTIONS_RUNNER_INPUT_JITCONFIG env var into the `runner`
# container; the stock agent `run.sh` picks it up (see
# autoscaling-runner-set.yaml). For the standalone `container:` workflow
# mode, GitHub Actions injects the job into the image.

ARG UBUNTU_VERSION=22.04
ARG RUNNER_VERSION=2.336.0
# Pinned to the CI toolchain per #199 (repo-wide @1.98.0 pinning). Preinstalling
# the exact version means `dtolnay/rust-toolchain@1.98.0` resolves locally on
# runner pods with no toolchain download; bump this together with the workflow
# pins when the CI toolchain moves.
ARG RUST_VERSION=1.98.0
ARG PROTOC_VERSION=29.3
# Optional apt mirror override for builds behind a slow link to
# archive.ubuntu.com (e.g. `--build-arg APT_MIRROR=mirrors.aliyun.com`).
# Empty by default so the image uses the stock Ubuntu mirrors.
ARG APT_MIRROR=
# Optional GitHub release proxy prefix for builds where github.com release
# downloads are slow or blocked (e.g. `--build-arg GITHUB_PROXY=https://ghfast.top/`).
# Applied to actions/runner, rustup, wasm tools, and protoc downloads.
# Empty by default so downloads go directly to github.com.
ARG GITHUB_PROXY=

###############################################################################
# Stage 1: base OS + system libraries
###############################################################################
FROM ubuntu:${UBUNTU_VERSION} AS base

# Prevent apt from prompting during image build.
ENV DEBIAN_FRONTEND=noninteractive \
    TZ=Etc/UTC

# Optionally swap in a faster apt mirror. Only touched when APT_MIRROR is
# non-empty, so the default build is unaffected. Uses http (not https)
# because ca-certificates is not installed yet at this point; aliyun and
# most Ubuntu mirrors serve over http just fine. amd64 sources point at
# archive/security.ubuntu.com; arm64 (ports) builds point at
# ports.ubuntu.com, so rewrite both.
ARG APT_MIRROR
RUN set -eux; \
    if [ -n "${APT_MIRROR}" ]; then \
        sed -i "s|http://archive.ubuntu.com/ubuntu|http://${APT_MIRROR}/ubuntu|g" /etc/apt/sources.list; \
        sed -i "s|http://security.ubuntu.com/ubuntu|http://${APT_MIRROR}/ubuntu|g" /etc/apt/sources.list; \
        sed -i "s|http://ports.ubuntu.com/ubuntu-ports|http://${APT_MIRROR}/ubuntu-ports|g" /etc/apt/sources.list; \
    fi

# Core build + runtime dependencies. Keep this list aligned with the
# `apt-get install` steps in .github/workflows/ci-*.yml so a job that
# passed on the bare runner also passes inside this image.
#
# Sources of truth:
#   ci-gate.yml     -> libopus-dev
#   ci-e2e.yml      -> jq sqlite3 lsof perl netcat-openbsd
#   ci-coverage.yml -> clang llvm  (bundled here; cheap and avoids a
#                                   separate coverage-only image)
#   baseline host toolchain -> build-essential pkg-config unzip
#   rusqlite        -> uses "bundled" feature, no libsqlite3-dev needed
RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential \
        pkg-config \
        unzip \
        ca-certificates \
        curl \
        git \
        git-lfs \
        jq \
        lsof \
        netcat-openbsd \
        perl \
        sqlite3 \
        clang \
        llvm \
        libopus-dev \
        libssl-dev \
        cmake \
        ninja-build \
        zlib1g-dev \
        xz-utils \
        file \
        openssh-client \
        sudo \
        python3 \
        python3-pip \
        python3-venv \
        lsb-release \
    && rm -rf /var/lib/apt/lists/*

# Actions cache otherwise falls back to gzip, which makes registry saves slow.
ARG GITHUB_PROXY
COPY install-zstd.sh /tmp/install-zstd.sh
RUN sh /tmp/install-zstd.sh && rm /tmp/install-zstd.sh

###############################################################################
# Stage 2: runner user + GitHub Actions runner agent
###############################################################################
FROM base AS runner-agent

ARG RUNNER_VERSION
ARG TARGETARCH
ARG GITHUB_PROXY

# GitHub Actions expects jobs to run as a non-root user named `runner`.
RUN useradd --create-home --uid 1001 --shell /bin/bash runner \
    && echo "runner ALL=(ALL) NOPASSWD:ALL" > /etc/sudoers.d/runner \
    && chmod 0440 /etc/sudoers.d/runner

# Install the GitHub Actions runner agent into /home/runner/actions-runner.
# The agent is a .NET app; `installdependencies.sh` pulls in the libicu /
# libssl runtime libs it needs. We map amd64 -> linux-x64; arm64 is wired
# for completeness even though the current fleet is x86_64 only.
# GITHUB_PROXY (e.g. https://ghfast.top/) can prefix the github.com URL in
# regions where release downloads are slow.
WORKDIR /home/runner/actions-runner
RUN set -eux; \
    case "$TARGETARCH" in \
        amd64) runner_arch=x64 ;; \
        arm64) runner_arch=arm64 ;; \
        *) echo "unsupported arch: $TARGETARCH" >&2; exit 1 ;; \
    esac; \
    runner_url="https://github.com/actions/runner/releases/download/v${RUNNER_VERSION}/actions-runner-linux-${runner_arch}-${RUNNER_VERSION}.tar.gz"; \
    if [ -n "${GITHUB_PROXY}" ]; then runner_url="${GITHUB_PROXY%/}/${runner_url}"; fi; \
    curl -fsSL -o runner.tar.gz "$runner_url"; \
    tar xzf runner.tar.gz; \
    rm runner.tar.gz; \
    ./bin/installdependencies.sh; \
    chown -R runner:runner /home/runner/actions-runner

###############################################################################
# Stage 3: Rust toolchain + wasm tools + protoc
###############################################################################
FROM runner-agent AS toolchain

ARG RUST_VERSION
ARG PROTOC_VERSION
ARG TARGETARCH
ARG GITHUB_PROXY
# Optional rustup mirror (e.g. https://mirrors.ustc.edu.cn/rust-static)
# for builds where static.rust-lang.org is slow/unreachable. Empty by
# default so rustup uses the official distribution server.
ARG RUSTUP_DIST_SERVER=
ARG RUSTUP_UPDATE_ROOT=

USER runner
WORKDIR /home/runner

# Rust toolchains. The default is the exact CI pin (1.98.0, see #199) so the
# `dtolnay/rust-toolchain@1.98.0` Setup Rust steps on ARC resolve locally.
# The MSRV toolchain (1.95.0) is preinstalled for the `rust_msrv` job; nightly
# for the coverage-web-wasm `@nightly` job. Components: clippy + rustfmt +
# rust-src (cargo doc/doctests) + llvm-tools-preview (coverage, ci-coverage.yml).
# Targets: wasm32-unknown-unknown + wasm32-wasip2 (wasm jobs). The Android
# Rust targets are deliberately absent: kotlin-echo-app-e2e needs the full
# JDK/Android SDK/KVM stack and stays on GitHub-hosted runners.
# RUSTUP_DIST_SERVER / RUSTUP_UPDATE_ROOT redirect rustup's channel-manifest
# sync to a mirror. The mirror URL is deliberately NOT hardcoded here: the
# ARGs pass through to ENV, and the deployment injects the value at build
# time (--build-arg, e.g. the incremental build on the image-build machine),
# so runner pods need no per-job env while this file stays neutral.
RUN set -eux; \
    if [ -n "${RUSTUP_DIST_SERVER}" ]; then export RUSTUP_DIST_SERVER; fi; \
    if [ -n "${RUSTUP_UPDATE_ROOT}" ]; then export RUSTUP_UPDATE_ROOT; fi; \
    curl --proto '=https' --tlsv1.2 -fsS https://sh.rustup.rs | sh -s -- -y \
        --default-toolchain "${RUST_VERSION}" \
        --profile minimal \
        --component clippy --component rust-src --component rustfmt \
        --component llvm-tools-preview; \
    . "$HOME/.cargo/env"; \
    rustup target add wasm32-unknown-unknown wasm32-wasip2; \
    rustup toolchain install nightly --profile minimal \
        --component llvm-tools-preview; \
    rustup +nightly target add wasm32-unknown-unknown wasm32-wasip2; \
    rustup toolchain install 1.95.0 --profile minimal \
        --component clippy --component rustfmt; \
    rustup +1.95.0 target add wasm32-unknown-unknown wasm32-wasip2

ENV PATH="/home/runner/.cargo/bin:${PATH}" \
    CARGO_HOME=/home/runner/.cargo \
    RUSTUP_HOME=/home/runner/.rustup \
    RUSTUP_DIST_SERVER=${RUSTUP_DIST_SERVER} \
    RUSTUP_UPDATE_ROOT=${RUSTUP_UPDATE_ROOT}

# Pre-built wasm tools, pinned to the versions the workflows require. The
# workflows are the single source of truth for these versions: they still
# install them on runners without this image (GitHub-hosted, macOS) and skip
# the install only when the binary reports the pinned version. This image ships
# those versions so jobs can reuse them without recompiling the tools. A
# mismatch fails the build
# loudly (see the assertions below) and tampered upstream releases fail it too
# because every archive is verified by sha256.
#   wasm-pack        v0.15.0   (ci-web / ci-gate / release workflows)
#   wasm-component-ld v0.5.26  (>= 0.5.22 is required to link the V2
#                               async-world guest fixture; CI pins 0.5.26)
#   wasm-tools       v1.253.0  (aligned with wasmparser 0.253, which wasmtime
#                               46.x embeds; used for local inspection and
#                               `component wit`, runtime validation is
#                               authoritative)
#   sccache          v0.18.0
# GITHUB_PROXY can prefix the github.com URLs in regions where release
# downloads are slow.
#
# sccache is pinned to 0.18.0 instead of the 0.10.0 of the wasm-tools era:
# 0.10.0 cannot start its daemon against the fleet's MinIO 2025-07 S3
# endpoint, which used to force every runner Pod to download 0.18.0 during
# startup. Installing it here removes that per-Pod download.
RUN set -eux; \
    install_binary() { \
        local name="$1" url="$2" sha="$3"; \
        local archive="/tmp/$name.tar.gz"; \
        local dir; dir="$(mktemp -d)"; \
        if [ -n "${GITHUB_PROXY}" ]; then url="${GITHUB_PROXY%/}/${url}"; fi; \
        curl --fail --location --retry 3 --retry-all-errors -o "$archive" "$url"; \
        echo "$sha  $archive" | sha256sum --check --status; \
        tar --extract --gzip --file "$archive" --strip-components=1 --directory "$dir"; \
        install --mode=0755 "$dir/$name" "$HOME/.cargo/bin/$name"; \
        rm -rf "$archive" "$dir"; \
    }; \
    case "$TARGETARCH" in \
        amd64) wasm_arch=x86_64-unknown-linux-musl; ld_arch=x86_64-linux; tools_arch=x86_64-linux ;; \
        arm64) wasm_arch=aarch64-unknown-linux-musl; ld_arch=aarch64-linux; tools_arch=aarch64-linux ;; \
        *) echo "unsupported arch: $TARGETARCH" >&2; exit 1 ;; \
    esac; \
    install_binary wasm-pack \
        "https://github.com/wasm-bindgen/wasm-pack/releases/download/v0.15.0/wasm-pack-v0.15.0-${wasm_arch}.tar.gz" \
        c09f971ecaed9a2efc80fdcea7a00ef6b53c7fadc8c57d1f61b53a6aa66b668a; \
    install_binary wasm-component-ld \
        "https://github.com/bytecodealliance/wasm-component-ld/releases/download/v0.5.26/wasm-component-ld-v0.5.26-${ld_arch}.tar.gz" \
        e5cee677052c0eb6b5d0605f85e3bbc46d076050cd3403406f49b3503f22622c; \
    install_binary wasm-tools \
        "https://github.com/bytecodealliance/wasm-tools/releases/download/v1.253.0/wasm-tools-1.253.0-${tools_arch}.tar.gz" \
        4e2898f7ca3bd0536218ed9b7b36ff7b86954c57ae0e6272fde69728cbe01088; \
    install_binary sccache \
        "https://github.com/mozilla/sccache/releases/download/v0.18.0/sccache-v0.18.0-x86_64-unknown-linux-musl.tar.gz" \
        45f1447fbe231e3037bde351ef70677dd212216c8d62ae7ca409fecc4d6acc89; \
    wasm-pack --version; wasm-component-ld --version; wasm-tools --version; \
    sccache --version; \
    # These must equal the versions the workflows pin (see the comment above);
    # drift here would make self-hosted runners validate something the other
    # runners do not.
    wasm-component-ld --version | grep -Fxq 'wasm-component-ld 0.5.26'; \
    wasm-tools --version | grep -Eq '^wasm-tools 1\.253\.0( \(.*\))?$'

# wasm-opt (Binaryen version_117). wasm-pack resolves wasm-opt through a code
# path (ureq) that ignores HTTP(S)_PROXY, so a job-time download of this
# tarball is unreliable on the fleet's proxied egress; it is therefore baked
# in. The archive ships a directory tree rather than a single binary, so it is
# extracted whole and wasm-opt is symlinked onto PATH.
# Pinned by sha256; the fleet is x86_64-only, and an unpinned arm64 checksum
# would be worse than failing loudly, so only amd64 is wired up.
ARG BINARYEN_VERSION=version_117
RUN set -eux; \
    case "$TARGETARCH" in \
        amd64) binaryen_arch=x86_64-linux ;; \
        *) echo "no pinned binaryen checksum for arch: $TARGETARCH" >&2; exit 1 ;; \
    esac; \
    archive=/tmp/binaryen.tar.gz; \
    url="https://github.com/WebAssembly/binaryen/releases/download/${BINARYEN_VERSION}/binaryen-${BINARYEN_VERSION}-${binaryen_arch}.tar.gz"; \
    if [ -n "${GITHUB_PROXY}" ]; then url="${GITHUB_PROXY%/}/${url}"; fi; \
    curl --fail --location --retry 3 --retry-all-errors -o "$archive" "$url"; \
    echo "3dc677006555b355ea2da5e82602065a161d5e83eaefd3f759afa00b96e83212  $archive" | sha256sum --check --status; \
    mkdir -p /home/runner/binaryen; \
    tar --extract --gzip --file "$archive" --strip-components=1 --directory /home/runner/binaryen; \
    ln -sf /home/runner/binaryen/bin/wasm-opt /home/runner/.cargo/bin/wasm-opt; \
    rm -f "$archive"; \
    wasm-opt --version

# protoc — the CI uses arduino/setup-protoc@v3 with vars.PROTOC_VERSION.
# We pin to a release from the protobuf repo and drop it on PATH.
# GITHUB_PROXY can prefix the github.com URL in regions where release
# downloads are slow.
USER root
RUN set -eux; \
    case "$TARGETARCH" in \
        amd64) protoc_arch=x86_64 ;; \
        arm64) protoc_arch=aarch_64 ;; \
        *) echo "unsupported arch: $TARGETARCH" >&2; exit 1 ;; \
    esac; \
    protoc_url="https://github.com/protocolbuffers/protobuf/releases/download/v${PROTOC_VERSION}/protoc-${PROTOC_VERSION}-linux-${protoc_arch}.zip"; \
    if [ -n "${GITHUB_PROXY}" ]; then protoc_url="${GITHUB_PROXY%/}/${protoc_url}"; fi; \
    curl -fsSL -o /tmp/protoc.zip "$protoc_url"; \
    unzip -q /tmp/protoc.zip -d /usr/local; \
    rm /tmp/protoc.zip; \
    protoc --version

# mold linker — optional fast ld for CI link steps. The image only ships the
# binary; activation is per-job via RUSTFLAGS="-C link-arg=-fuse-ld=mold"
# (injected by the runner manifest), so local/other uses are unaffected.
# The tarball ships bin/mold + bin/ld.mold (the name gcc/clang look up for
# -fuse-ld=mold), extracted into /usr/local.
# GITHUB_PROXY can prefix the github.com URL in regions where release
# downloads are slow.
RUN set -eux; \
    mold_url="https://github.com/rui314/mold/releases/download/v2.40.4/mold-2.40.4-x86_64-linux.tar.gz"; \
    if [ -n "${GITHUB_PROXY}" ]; then mold_url="${GITHUB_PROXY%/}/${mold_url}"; fi; \
    curl -fsSL --retry 3 --retry-all-errors -o /tmp/mold.tar.gz "$mold_url"; \
    echo "4c999e19ffa31afa5aa429c679b665d5e2ca5a6b6832ad4b79668e8dcf3d8ec1  /tmp/mold.tar.gz" | sha256sum --check --status; \
    tar xzf /tmp/mold.tar.gz --strip-components=1 -C /usr/local; \
    rm /tmp/mold.tar.gz; \
    mold --version; \
    # prove -fuse-ld=mold resolves through the compiler driver
    echo 'int main(void){return 0;}' | cc -x c - -fuse-ld=mold -o /tmp/moldtest && /tmp/moldtest && rm /tmp/moldtest

###############################################################################
# Stage 3.5: hosted toolcache + Android SDK
#
# actions/setup-python, setup-node, setup-java honor $RUNNER_TOOL_CACHE:
# with a matching pre-populated entry they SKIP the download entirely.
# android-actions/setup-android honors $ANDROID_SDK_ROOT the same way.
# Without this, every job re-downloads its toolchain from github.com —
# 5-10 minutes per job from slow links. Layout follows actions/tool-cache
# convention: <tool>/<semver>/<arch>/ plus an <arch>.complete marker file
# INSIDE <semver>/ (e.g. Python/3.11.16/x64.complete). The marker lives
# next to the <arch>/ dir and its name is exactly `<arch>.complete` —
# not `<semver>.<arch>.complete` — or tc.find() misses it and the action
# silently re-downloads.
###############################################################################
FROM toolchain AS toolcache

ARG GITHUB_PROXY
# Pinned toolcache versions. Bump to upgrade; workflows ask for loose ranges
# ("3.11", "24", "17") so any patch version here satisfies them.
ARG PY311=3.11.16
ARG PY311_TAG=3.11.16-31661435402
ARG PY313=3.13.15
ARG PY313_TAG=3.13.15-31064747964
ARG NODE_VERSION=24.19.0
# Adoptium release string; directory name in the toolcache must be a semver
# that satisfies "17".
ARG JDK_VERSION=17.0.20+8
# cmdline-tools build id from dl.google.com. Pinned to 12266719 (16.0) to
# match android-actions/setup-android's default cmdline-tools-version — with
# an older build the action reports "Wrong version in preinstalled sdkmanager"
# and re-downloads the whole toolchain at job time (~11 min, retry included).
ARG CMDLINE_TOOLS=12266719
# Gradle distribution + KtLint dependency versions, pinned to what
# packages/actr-kotlin resolves at job time (gradle/wrapper/gradle-wrapper.properties
# and build.gradle.kts). Pre-seeding both into the Gradle user home removes the
# per-job distribution + KtLint dependency downloads.
ARG GRADLE_VERSION=9.2.1
ARG KTLINT_VERSION=1.5.0
# Optional archive download mirror. The wrapper's canonical URL still selects
# the cache slot, and the archive must match the checksum below.
ARG GRADLE_DIST_PROXY=
# SHA-256 of the Gradle archive named by the wrapper. Verify the image-build
# download too; a pre-seeded distribution bypasses the wrapper's download-time
# checksum check. Update this together with the wrapper property when the
# Gradle version changes.
ARG GRADLE_DIST_SHA256=72f44c9f8ebcb1af43838f45ee5c4aa9c5444898b3468ab3f4af7b6076c5bc3f
# Version of the Gradle KtLint plugin used by packages/actr-kotlin/build.gradle.kts.
ARG KTLINT_GRADLE_VERSION=12.1.2

USER root

ENV RUNNER_TOOL_CACHE=/opt/hostedtoolcache \
    ANDROID_HOME=/usr/local/lib/android/sdk \
    ANDROID_SDK_ROOT=/usr/local/lib/android/sdk

# Python 3.11 + 3.13 into the toolcache (prebuilt tarballs from the same
# actions/python-versions releases setup-python downloads).
RUN set -eux; \
    install_python() { \
        local ver="$1" tag="$2"; \
        local dest="$RUNNER_TOOL_CACHE/Python/${ver}/x64"; \
        local url="https://github.com/actions/python-versions/releases/download/${tag}/python-${ver}-linux-22.04-x64.tar.gz"; \
        if [ -n "${GITHUB_PROXY}" ]; then url="${GITHUB_PROXY%/}/${url}"; fi; \
        mkdir -p "$dest"; \
        curl -fsSL --retry 3 --retry-all-errors "$url" | tar xz -C "$dest"; \
        touch "$RUNNER_TOOL_CACHE/Python/${ver}/x64.complete"; \
        # setup-python's own install path creates python/pip aliases on top
        # of the tarball (bin/ ships python3 only); replicate them so
        # scripts calling `python` / `pip` work against the cached copy.
        ln -sf python3 "$dest/bin/python"; \
        ln -sf pip3 "$dest/bin/pip"; \
        "$dest/bin/python3" --version; "$dest/bin/python" --version; \
    }; \
    install_python "$PY311" "$PY311_TAG"; \
    install_python "$PY313" "$PY313_TAG"

# Node 24 into the toolcache (nodejs.org; no GH proxy redirect needed there,
# but allow it for parity in case the arg points at a mirror of nodejs.org).
RUN set -eux; \
    dest="$RUNNER_TOOL_CACHE/node/${NODE_VERSION}/x64"; \
    mkdir -p "$dest"; \
    curl -fsSL --retry 3 --retry-all-errors "https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-linux-x64.tar.gz" \
      | tar xz --strip-components=1 -C "$dest"; \
    touch "$RUNNER_TOOL_CACHE/node/${NODE_VERSION}/x64.complete"; \
    "$dest/bin/node" --version

# Temurin JDK 17 into the toolcache (adoptium GitHub release; GITHUB_PROXY
# applies). The extracted JDK also drives sdkmanager below.
RUN set -eux; \
    dest="$RUNNER_TOOL_CACHE/Java_Temurin-Hotspot_jdk/${JDK_VERSION}/x64"; \
    jdk_ver_noplus="$(echo "$JDK_VERSION" | tr '+' '_')"; \
    jdk_ver_url="$(echo "$JDK_VERSION" | sed 's/+/%2B/')"; \
    url="https://github.com/adoptium/temurin17-binaries/releases/download/jdk-${jdk_ver_url}/OpenJDK17U-jdk_x64_linux_hotspot_${jdk_ver_noplus}.tar.gz"; \
    if [ -n "${GITHUB_PROXY}" ]; then url="${GITHUB_PROXY%/}/${url}"; fi; \
    mkdir -p "$dest"; \
    curl -fsSL --retry 3 --retry-all-errors "$url" | tar xz --strip-components=1 -C "$dest"; \
    touch "$RUNNER_TOOL_CACHE/Java_Temurin-Hotspot_jdk/${JDK_VERSION}/x64.complete"; \
    "$dest/bin/java" -version

# Android cmdline-tools + the exact packages ci-kotlin installs
# (platforms;android-34, build-tools;34.0.0). setup-android then finds a
# valid SDK at $ANDROID_SDK_ROOT and only runs sdkmanager for missing
# packages — of which there are none. dl.google.com downloaded directly
# (no GitHub proxy applies).
RUN set -eux; \
    mkdir -p "$ANDROID_SDK_ROOT/cmdline-tools"; \
    curl -fsSL --retry 3 --retry-all-errors -o /tmp/cmdline-tools.zip \
      "https://dl.google.com/android/repository/commandlinetools-linux-${CMDLINE_TOOLS}_latest.zip"; \
    unzip -q /tmp/cmdline-tools.zip -d "$ANDROID_SDK_ROOT/cmdline-tools"; \
    mv "$ANDROID_SDK_ROOT/cmdline-tools/cmdline-tools" "$ANDROID_SDK_ROOT/cmdline-tools/latest"; \
    rm /tmp/cmdline-tools.zip; \
    export JAVA_HOME="$RUNNER_TOOL_CACHE/Java_Temurin-Hotspot_jdk/${JDK_VERSION}/x64"; \
    sdkm="$ANDROID_SDK_ROOT/cmdline-tools/latest/bin/sdkmanager"; \
    yes | "$sdkm" --licenses >/dev/null; \
    "$sdkm" --install "platforms;android-34" "build-tools;34.0.0" "build-tools;35.0.0" "platform-tools"; \
    "$sdkm" --list_installed | grep -E 'android-34|build-tools;34|build-tools;35'

# Pre-seed Gradle and the KtLint dependencies so the Kotlin Binding Test does
# not download them at job time. GRADLE_USER_HOME is set explicitly so its
# location does not drift with HOME handling. The canonical wrapper URL selects
# the cache slot; GRADLE_DIST_PROXY may change only the download source.
#
# The Gradle wrapper keys wrapper/dists off MD5(distributionUrl), so a
# manually placed copy under a different subdir is never reused — the wrapper
# re-downloads regardless of any gradle on PATH (confirmed: the Kotlin job
# still pulled the dist for ~10 min). Stage the distribution at the exact
# wrapper path: wrapper/dists/<distName>/<base36-md5>/<dist>/{<dist>.ok,<dist>.lck}
# so ./gradlew resolves it with zero download. GRADLE_DIST_MD5 is the
# base36-encoded MD5(distributionUrl); the build checks it below.
ENV GRADLE_USER_HOME=/home/runner/.gradle
ARG GRADLE_DIST_MD5=2t0n5ozlw9xmuyvbp7dnzaxug
COPY prepare-gradle-cache.py /usr/local/bin/prepare-gradle-cache.py
COPY ktlint-preseed.gradle /tmp/ktlint-preseed/build.gradle
RUN set -eux; \
    wrapper_url="https://services.gradle.org/distributions/gradle-${GRADLE_VERSION}-bin.zip"; \
    dist_url="$wrapper_url"; \
    if [ -n "${GRADLE_DIST_PROXY}" ]; then dist_url="${GRADLE_DIST_PROXY%/}/gradle-${GRADLE_VERSION}-bin.zip"; fi; \
    dist_hash="$(python3 /usr/local/bin/prepare-gradle-cache.py --distribution-url "$wrapper_url" --print-cache-key)"; \
    test "$dist_hash" = "$GRADLE_DIST_MD5"; \
    dist_dir="$GRADLE_USER_HOME/wrapper/dists/gradle-${GRADLE_VERSION}-bin/${GRADLE_DIST_MD5}"; \
    mkdir -p /tmp/gradle-dist "$dist_dir"; \
    curl -fsSL --retry 3 --retry-all-errors -o /tmp/gradle-dist/gradle.zip "$dist_url"; \
    echo "${GRADLE_DIST_SHA256}  /tmp/gradle-dist/gradle.zip" | sha256sum --check --status; \
    unzip -q /tmp/gradle-dist/gradle.zip -d "$dist_dir"; \
    # Marker name matters byte-for-byte: this wrapper version derives it from
    # the local zip name, i.e. "gradle-<ver>-bin.zip.ok" (presence check only,
    # content is ignored — verified with a local wrapper experiment). A marker
    # without the .zip infix, or one holding the sha256, is never recognized
    # and every job re-downloads the distribution. Preparation and the wrapper
    # use the same lock when staging or validating this directory.
    touch "$dist_dir/gradle-${GRADLE_VERSION}-bin.zip.ok"; \
    rm -rf /tmp/gradle-dist; \
    export JAVA_HOME="$RUNNER_TOOL_CACHE/Java_Temurin-Hotspot_jdk/${JDK_VERSION}/x64"; \
    "$dist_dir/gradle-${GRADLE_VERSION}/bin/gradle" --version; \
    "$dist_dir/gradle-${GRADLE_VERSION}/bin/gradle" \
      -p /tmp/ktlint-preseed \
      --no-daemon \
      -PktlintVersion="${KTLINT_VERSION}" \
      -PktlintGradleVersion="${KTLINT_GRADLE_VERSION}" \
      resolveKtlint; \
    rm -rf /tmp/ktlint-preseed

# Everything above installed as root; hand the toolcache + SDK + Gradle home
# to the runner user so job-time sdkmanager/Gradle writes also work.
RUN chown -R runner:runner /opt/hostedtoolcache /usr/local/lib/android "$GRADLE_USER_HOME"

###############################################################################
# Stage 4: final image
###############################################################################
FROM toolcache AS final

# Re-declare toolcache version ARGs: Docker ARGs do NOT cross stage boundaries,
# so without these the ENV PATH and sanity RUN below would need hardcoded
# versions. Keeping them as ARG lets `--build-arg NODE_VERSION=...` propagate
# all the way through.
ARG PY311=3.11.16
ARG PY313=3.13.15
ARG NODE_VERSION=24.19.0
ARG JDK_VERSION=17.0.20+8
ARG GRADLE_VERSION=9.2.1
ARG KTLINT_VERSION=1.5.0
ARG KTLINT_GRADLE_VERSION=12.1.2
ARG GRADLE_DIST_MD5=2t0n5ozlw9xmuyvbp7dnzaxug

# Default user: runner (non-root, matches GitHub Actions convention).
USER runner
WORKDIR /home/runner

# Container-mode signal handling + stdout logging, matching the official
# actions-runner image conventions (RUNNER_MANUALLY_TRAP_SIG lets run.sh
# forward SIGINT/SIGTERM to the listener for graceful pod shutdown).
ENV RUNNER_MANUALLY_TRAP_SIG=1 \
    ACTIONS_RUNNER_PRINT_LOG_TO_STDOUT=1

# GitHub-hosted runners keep node/java on PATH system-wide; jobs and Rust
# tests that shell out to `npm`/`java` directly (without a setup-* step,
# e.g. crates/actr-cli/tests/typescript_gen_scenarios.rs runs `npm install`) depend on
# that. Put the toolcache bins on PATH so this image matches. setup-*
# steps still hit the toolcache for version resolution as usual.
#
# The Python toolcache bin comes first because the system python3 in this
# image is 3.10 while CI scripts (for example the e2e/package-runtime-echo
# script, which uses the stdlib tomllib module) require 3.11+. This replaces
# the `export PATH` the deployment used to run in its startup script.
ENV PATH="/opt/hostedtoolcache/Python/${PY311}/x64/bin:/opt/hostedtoolcache/node/${NODE_VERSION}/x64/bin:/opt/hostedtoolcache/Java_Temurin-Hotspot_jdk/${JDK_VERSION}/x64/bin:${PATH}"

# Container entrypoint: runtime-only preparation (pre-start sccache) followed
# by the runner agent. The script also forwards an explicit command, so
# `docker run <image> <cmd>` and the `container:` workflow mode keep working.
USER root
COPY install-wasm-bindgen.sh /tmp/install-wasm-bindgen.sh
RUN WASM_BINDGEN_INSTALL_DIR=/home/runner/.cargo/bin sh /tmp/install-wasm-bindgen.sh \
    && rm /tmp/install-wasm-bindgen.sh
COPY entrypoint.sh /usr/local/bin/runner-entrypoint.sh
RUN chmod 0755 /usr/local/bin/runner-entrypoint.sh
USER runner

# Actions cache integration. The fleet serves Actions file caches from an
# in-cluster v2 cache server, so the worker is patched to keep job cache
# traffic off the hosted v2 results endpoint. The patch is a byte-level
# rename of one UTF-16LE literal and asserts that it applied exactly once;
# see patch-runner-dll.py and README.md ("Actions cache integration").
COPY --chown=runner:runner patch-runner-dll.py /tmp/patch-runner-dll.py
RUN python3 /tmp/patch-runner-dll.py && rm -f /tmp/patch-runner-dll.py

# Sanity-check the toolchain is on PATH at build time; a missing binary
# fails the build instead of failing a CI job later.
RUN set -eux; \
    rustc --version; cargo --version; rustfmt --version; cargo-clippy --version; \
    protoc --version; git --version; jq --version; sqlite3 --version; \
    python3 --version; pip3 --version; lsb_release -d; \
    node --version; npm --version; java -version; \
    sccache --version; mold --version; wasm-opt --version; zstd --version; \
    rustup run nightly rustc --version; \
    rustup component list --installed | grep -qE 'llvm-tools'; \
    rustup target list --installed | grep -q wasm32-unknown-unknown; \
    rustup target list --installed | grep -q wasm32-wasip2; \
    \
    # sccache must be the pinned 0.18.0: the fleet's MinIO endpoint rejects a
    # 0.10.0 daemon startup, and such a regression only surfaces at job time.
    sccache --version | grep -Fxq 'sccache 0.18.0'; \
    wasm-pack --version | grep -Fxq 'wasm-pack 0.15.0'; \
    # tomllib is the 3.11+ stdlib module the e2e/package-runtime-echo script
    # needs, so python3 on PATH must be the toolcache interpreter.
    python3 -c 'import tomllib'; \
    # Fail the build if the Actions cache worker patch silently did not apply.
    python3 -c "from pathlib import Path; d = Path('/home/runner/actions-runner/bin/Runner.Worker.dll').read_bytes(); assert d.count('ACTIONS_RESULTS_URL'.encode('utf-16-le')) == 0, 'cache patch missing'; assert d.count('ACTIONS_RESULTS_ORL'.encode('utf-16-le')) == 1, 'cache patch malformed'; print('cache patch verified')"; \
    \
    # toolcache: every entry setup-* should find must exist WITH its
    # .complete marker, else the action silently re-downloads at job time.
    /opt/hostedtoolcache/Python/${PY311}/x64/bin/python3 --version; \
    /opt/hostedtoolcache/Python/${PY311}/x64/bin/python --version; \
    /opt/hostedtoolcache/Python/${PY311}/x64/bin/pip --version; \
    /opt/hostedtoolcache/Python/${PY313}/x64/bin/python3 --version; \
    /opt/hostedtoolcache/node/${NODE_VERSION}/x64/bin/node --version; \
    /opt/hostedtoolcache/Java_Temurin-Hotspot_jdk/${JDK_VERSION}/x64/bin/java -version; \
    test -f /opt/hostedtoolcache/Python/${PY311}/x64.complete; \
    test -f /opt/hostedtoolcache/Python/${PY313}/x64.complete; \
    test -f /opt/hostedtoolcache/node/${NODE_VERSION}/x64.complete; \
    test -f "/opt/hostedtoolcache/Java_Temurin-Hotspot_jdk/${JDK_VERSION}/x64.complete"; \
    JAVA_HOME=/opt/hostedtoolcache/Java_Temurin-Hotspot_jdk/${JDK_VERSION}/x64 \
      "$ANDROID_SDK_ROOT/cmdline-tools/latest/bin/sdkmanager" --list_installed \
      | grep -qE 'platforms;android-34|build-tools;34.0.0'; \
    # Gradle distribution and KtLint dependency metadata are pre-seeded;
    # otherwise the job re-downloads them on every ephemeral pod.
    test -x "$GRADLE_USER_HOME/wrapper/dists/gradle-${GRADLE_VERSION}-bin/${GRADLE_DIST_MD5}/gradle-${GRADLE_VERSION}/bin/gradle"; \
    test -f "$GRADLE_USER_HOME/wrapper/dists/gradle-${GRADLE_VERSION}-bin/${GRADLE_DIST_MD5}/gradle-${GRADLE_VERSION}-bin.zip.ok"; \
    test -d "$GRADLE_USER_HOME/caches/modules-2/files-2.1/com.pinterest.ktlint/ktlint-cli/${KTLINT_VERSION}"; \
    test -d "$GRADLE_USER_HOME/caches/modules-2/files-2.1/org.jlleitschuh.gradle/ktlint-gradle/${KTLINT_GRADLE_VERSION}"

# Both build paths use the same bootstrap and toolcache validation.
COPY --chown=runner:runner check-runner-bootstrap.sh /tmp/check-runner-bootstrap.sh
RUN sh /tmp/check-runner-bootstrap.sh "${PY311}" "${PY313}" "${NODE_VERSION}" "${JDK_VERSION}" \
    "https://services.gradle.org/distributions/gradle-${GRADLE_VERSION}-bin.zip" \
    && rm /tmp/check-runner-bootstrap.sh

# The runner agent is registered at container start: ARC injects the JIT
# config via ACTIONS_RUNNER_INPUT_JITCONFIG and run.sh consumes it. The
# entrypoint additionally tolerates an explicit command, so the standalone
# `container:` workflow mode can still drive the image directly.
ENTRYPOINT ["/usr/local/bin/runner-entrypoint.sh"]
LABEL org.opencontainers.image.title="actrium-runner" \
      org.opencontainers.image.description="Actrium self-hosted GitHub Actions runner (Linux)" \
      org.opencontainers.image.source="https://github.com/Actrium/actrium" \
      org.opencontainers.image.licenses="Apache-2.0"
