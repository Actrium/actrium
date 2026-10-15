# syntax=docker/dockerfile:1
# Actrium Linux x86_64 CI runner. Build context: tools/ci/runner/.
# Supply a digest-pinned BASE_IMAGE and BUILDKIT_SYNTAX to use registry mirrors.
ARG BASE_IMAGE=ubuntu:22.04
FROM ${BASE_IMAGE} AS base
ARG TARGETARCH
ARG APT_MIRROR=
ENV DEBIAN_FRONTEND=noninteractive TZ=Etc/UTC
# Apt's download and index caches remain in BuildKit, outside the image.
RUN --mount=type=cache,id=actrium-runner-apt,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,id=actrium-runner-apt-lists,target=/var/lib/apt/lists,sharing=locked \
    set -eu; \
    test "$TARGETARCH" = amd64; \
    if [ -n "$APT_MIRROR" ]; then \
        sed -i "s|http://archive.ubuntu.com/ubuntu|http://$APT_MIRROR/ubuntu|g; s|http://security.ubuntu.com/ubuntu|http://$APT_MIRROR/ubuntu|g" /etc/apt/sources.list; \
    fi; \
    rm -f /etc/apt/apt.conf.d/docker-clean; \
    echo 'Binary::apt::APT::Keep-Downloaded-Packages "true";' > /etc/apt/apt.conf.d/keep-cache; \
    apt-get update; \
    apt-get install -y --no-install-recommends \
        build-essential pkg-config unzip ca-certificates curl git git-lfs jq lsof \
        netcat-openbsd perl sqlite3 clang llvm libopus-dev libssl-dev cmake \
        ninja-build zlib1g-dev xz-utils file openssh-client sudo python3 python3-pip \
        python3-venv lsb-release; \
    useradd --create-home --uid 1001 --shell /bin/bash runner; \
    echo 'runner ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/runner; \
    chmod 0440 /etc/sudoers.d/runner; \
    install -d -o 1001 -g 1001 /opt/hostedtoolcache /usr/local/lib/android \
        /home/runner/actions-runner /home/runner/.cargo /home/runner/.rustup
ENV HOME=/home/runner \
    CARGO_HOME=/home/runner/.cargo RUSTUP_HOME=/home/runner/.rustup \
    RUNNER_TOOL_CACHE=/opt/hostedtoolcache \
    ANDROID_HOME=/usr/local/lib/android/sdk ANDROID_SDK_ROOT=/usr/local/lib/android/sdk \
    GRADLE_USER_HOME=/home/runner/.gradle \
    PATH=/home/runner/.cargo/bin:${PATH}

# Stable language/SDK caches do not depend on the Rust stage.
FROM base AS python-cache
ARG GITHUB_PROXY=
COPY runner-install-lib.sh install-toolcache.sh /tmp/runner-install/
COPY versions/toolcache.env /tmp/runner-install/versions/toolcache.env
USER runner
RUN --mount=type=cache,id=actrium-runner-downloads,target=/var/cache/runner-downloads,uid=1001,gid=1001,sharing=shared \
    RUNNER_DOWNLOAD_CACHE=/var/cache/runner-downloads sh /tmp/runner-install/install-toolcache.sh python

FROM base AS node-cache
COPY runner-install-lib.sh install-toolcache.sh /tmp/runner-install/
COPY versions/toolcache.env /tmp/runner-install/versions/toolcache.env
USER runner
RUN --mount=type=cache,id=actrium-runner-downloads,target=/var/cache/runner-downloads,uid=1001,gid=1001,sharing=shared \
    RUNNER_DOWNLOAD_CACHE=/var/cache/runner-downloads sh /tmp/runner-install/install-toolcache.sh node

FROM base AS java-cache
ARG GITHUB_PROXY=
COPY runner-install-lib.sh install-toolcache.sh /tmp/runner-install/
COPY versions/toolcache.env /tmp/runner-install/versions/toolcache.env
USER runner
RUN --mount=type=cache,id=actrium-runner-downloads,target=/var/cache/runner-downloads,uid=1001,gid=1001,sharing=shared \
    RUNNER_DOWNLOAD_CACHE=/var/cache/runner-downloads sh /tmp/runner-install/install-toolcache.sh java

FROM java-cache AS android-cache
RUN --mount=type=cache,id=actrium-runner-downloads,target=/var/cache/runner-downloads,uid=1001,gid=1001,sharing=shared \
    RUNNER_DOWNLOAD_CACHE=/var/cache/runner-downloads sh /tmp/runner-install/install-toolcache.sh android

FROM java-cache AS gradle-cache
ARG GRADLE_DIST_PROXY=
COPY prepare-gradle-cache.py ktlint-preseed.gradle /tmp/runner-install/
RUN --mount=type=cache,id=actrium-runner-downloads,target=/var/cache/runner-downloads,uid=1001,gid=1001,sharing=shared \
    RUNNER_DOWNLOAD_CACHE=/var/cache/runner-downloads sh /tmp/runner-install/install-toolcache.sh gradle \
    && rm -rf "$GRADLE_USER_HOME/daemon"

FROM base AS rust-toolchain
ARG RUST_VERSION
ARG RUSTUP_DIST_SERVER=https://static.rust-lang.org
ARG RUSTUP_UPDATE_ROOT=https://static.rust-lang.org/rustup
ENV RUSTUP_DIST_SERVER=${RUSTUP_DIST_SERVER} RUSTUP_UPDATE_ROOT=${RUSTUP_UPDATE_ROOT}
COPY runner-install-lib.sh install-rust.sh /tmp/runner-install/
COPY versions/rust.env /tmp/runner-install/versions/rust.env
USER runner
# Only downloaded archives are cached; extraction and installation share a filesystem.
# Installed toolchains stay in the image and work without network access.
RUN --mount=type=cache,id=actrium-runner-rust-downloads,target=/home/runner/.rustup/downloads,uid=1001,gid=1001,sharing=locked \
    --mount=type=cache,id=actrium-runner-downloads,target=/var/cache/runner-downloads,uid=1001,gid=1001,sharing=shared \
    RUNNER_DOWNLOAD_CACHE=/var/cache/runner-downloads sh /tmp/runner-install/install-rust.sh

FROM base AS wasm-tools
ARG GITHUB_PROXY=
COPY runner-install-lib.sh install-image-tools.sh install-wasm-bindgen.sh /tmp/runner-install/
COPY versions/wasm.env /tmp/runner-install/versions/wasm.env
USER runner
RUN --mount=type=cache,id=actrium-runner-downloads,target=/var/cache/runner-downloads,uid=1001,gid=1001,sharing=shared \
    RUNNER_DOWNLOAD_CACHE=/var/cache/runner-downloads sh /tmp/runner-install/install-image-tools.sh wasm

FROM base AS native-tools
ARG GITHUB_PROXY=
ARG ZSTD_BUILD_JOBS=
COPY runner-install-lib.sh install-image-tools.sh install-zstd.sh /tmp/runner-install/
COPY versions/native.env /tmp/runner-install/versions/native.env
RUN --mount=type=cache,id=actrium-runner-downloads,target=/var/cache/runner-downloads,uid=1001,gid=1001,sharing=shared \
    RUNNER_DOWNLOAD_CACHE=/var/cache/runner-downloads sh /tmp/runner-install/install-image-tools.sh native

FROM base AS runner-agent
ARG GITHUB_PROXY=
COPY runner-install-lib.sh install-agent.sh patch-runner-dll.py /tmp/runner-install/
COPY versions/agent.env /tmp/runner-install/versions/agent.env
USER runner
RUN --mount=type=cache,id=actrium-runner-downloads,target=/var/cache/runner-downloads,uid=1001,gid=1001,sharing=shared \
    RUNNER_DOWNLOAD_CACHE=/var/cache/runner-downloads sh /tmp/runner-install/install-agent.sh
USER root
RUN --mount=type=cache,id=actrium-runner-apt,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,id=actrium-runner-apt-lists,target=/var/lib/apt/lists,sharing=locked \
    /home/runner/actions-runner/bin/installdependencies.sh

# Copy only installed outputs. No old Rust toolchain or ownership-copy layer
# is inherited from a previously published runner image.
FROM runner-agent AS final
COPY --from=python-cache --chown=1001:1001 /opt/hostedtoolcache/ /opt/hostedtoolcache/
COPY --from=node-cache --chown=1001:1001 /opt/hostedtoolcache/ /opt/hostedtoolcache/
COPY --from=java-cache --chown=1001:1001 /opt/hostedtoolcache/ /opt/hostedtoolcache/
COPY --from=android-cache --chown=1001:1001 /usr/local/lib/android/ /usr/local/lib/android/
COPY --from=gradle-cache --chown=1001:1001 /home/runner/.gradle/ /home/runner/.gradle/
COPY --from=rust-toolchain --chown=1001:1001 /home/runner/.cargo/ /home/runner/.cargo/
COPY --from=rust-toolchain --chown=1001:1001 /home/runner/.rustup/ /home/runner/.rustup/
COPY --from=wasm-tools --chown=1001:1001 /home/runner/.cargo/bin/ /home/runner/.cargo/bin/
COPY --from=wasm-tools --chown=1001:1001 /home/runner/binaryen/ /home/runner/binaryen/
COPY --from=native-tools /usr/local/ /usr/local/
COPY --chmod=0755 entrypoint.sh /usr/local/bin/runner-entrypoint.sh
COPY prepare-gradle-cache.py /usr/local/bin/prepare-gradle-cache.py
COPY check-runner-bootstrap.sh /usr/local/share/actrium-runner/
COPY versions/ /usr/local/share/actrium-runner/versions/
ARG RUST_VERSION
ARG RUSTUP_DIST_SERVER=https://static.rust-lang.org
ARG RUSTUP_UPDATE_ROOT=https://static.rust-lang.org/rustup
ENV RUSTUP_DIST_SERVER=${RUSTUP_DIST_SERVER} RUSTUP_UPDATE_ROOT=${RUSTUP_UPDATE_ROOT} \
    JAVA_HOME=/opt/hostedtoolcache/current/java \
    PATH=/opt/hostedtoolcache/current/python/bin:/opt/hostedtoolcache/current/node/bin:/opt/hostedtoolcache/current/java/bin:${PATH} \
    RUNNER_MANUALLY_TRAP_SIG=1 ACTIONS_RUNNER_PRINT_LOG_TO_STDOUT=1
USER runner
WORKDIR /home/runner
# Remove diagnostics before committing the layer, rather than hiding them later.
RUN sh /usr/local/share/actrium-runner/check-runner-bootstrap.sh \
    && rm -rf "$HOME/actions-runner/_diag" "$GRADLE_USER_HOME/daemon"
ENTRYPOINT ["/usr/local/bin/runner-entrypoint.sh"]
LABEL org.opencontainers.image.title="actrium-runner" \
      org.opencontainers.image.description="Actrium self-hosted GitHub Actions runner (Linux)" \
      org.opencontainers.image.source="https://github.com/Actrium/actrium" \
      org.opencontainers.image.licenses="Apache-2.0"
