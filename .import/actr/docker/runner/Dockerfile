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
#   docker build -t actrium-runner:dev -f docker/runner/Dockerfile docker/runner/
#
# Under ARC (gha-runner-scale-set), the controller injects the JIT runner
# config as the ACTIONS_RUNNER_INPUT_JITCONFIG env var into the `runner`
# container; the stock agent `run.sh` picks it up (see
# autoscaling-runner-set.yaml). For the standalone `container:` workflow
# mode, GitHub Actions injects the job into the image.

ARG UBUNTU_VERSION=22.04
ARG RUNNER_VERSION=2.336.0
ARG RUST_VERSION=stable
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
    && rm -rf /var/lib/apt/lists/*

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

# Rust toolchain. Default = stable (latest), which satisfies the workspace
# MSRV (Cargo.toml rust-version = "1.95") and matches the `@stable` jobs that
# make up most of the CI. `rust_msrv` jobs still pin @1.95.0 via dtolnay
# (historical toolchain, not preinstalled). Nightly is preinstalled for the
# coverage-web-wasm `@nightly` job. Components: clippy + rustfmt + rust-src
# (cargo doc/doctests) + llvm-tools-preview (coverage, ci-coverage.yml).
# Targets: wasm32-unknown-unknown + wasm32-wasip2 (wasm jobs). The Android
# Rust targets are deliberately absent: kotlin-echo-app-e2e needs the full
# JDK/Android SDK/KVM stack and stays on GitHub-hosted runners.
# RUSTUP_DIST_SERVER / RUSTUP_UPDATE_ROOT redirect to a mirror when set.
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
    rustup +nightly target add wasm32-unknown-unknown wasm32-wasip2

ENV PATH="/home/runner/.cargo/bin:${PATH}" \
    CARGO_HOME=/home/runner/.cargo \
    RUSTUP_HOME=/home/runner/.rustup

# Pre-built wasm tools the CI downloads every run today (ci-gate.yml
# "Install prebuilt WebAssembly tools"). Pin them by sha256 so a
# tampered upstream release fails the build loudly.
#   wasm-pack        v0.15.0
#   wasm-component-ld v0.5.26
#   wasm-tools       v1.253.0
# GITHUB_PROXY can prefix the github.com URLs in regions where release
# downloads are slow.
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
    wasm-pack --version; wasm-component-ld --version; wasm-tools --version

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

###############################################################################
# Stage 4: final image
###############################################################################
FROM toolchain AS final

# Default user: runner (non-root, matches GitHub Actions convention).
USER runner
WORKDIR /home/runner

# Container-mode signal handling + stdout logging, matching the official
# actions-runner image conventions (RUNNER_MANUALLY_TRAP_SIG lets run.sh
# forward SIGINT/SIGTERM to the listener for graceful pod shutdown).
ENV RUNNER_MANUALLY_TRAP_SIG=1 \
    ACTIONS_RUNNER_PRINT_LOG_TO_STDOUT=1

# Sanity-check the toolchain is on PATH at build time; a missing binary
# fails the build instead of failing a CI job later.
RUN set -eux; \
    rustc --version; cargo --version; rustfmt --version; cargo-clippy --version; \
    protoc --version; git --version; jq --version; sqlite3 --version; \
    rustup run nightly rustc --version; \
    rustup component list --installed | grep -qE 'llvm-tools'; \
    rustup target list --installed | grep -q wasm32-unknown-unknown; \
    rustup target list --installed | grep -q wasm32-wasip2

# The runner agent is registered at container start (ARC injects the JIT
# config via env, or config.sh runs manually); no ENTRYPOINT here so both
# ARC and `container:` workflow modes can drive it.
LABEL org.opencontainers.image.title="actrium-runner" \
      org.opencontainers.image.description="Actrium self-hosted GitHub Actions runner (Linux)" \
      org.opencontainers.image.source="https://github.com/Actrium/actr" \
      org.opencontainers.image.licenses="Apache-2.0"
