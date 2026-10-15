#!/bin/sh
# Shared download handling; only download archives belong in the cache mount.

github_url() {
    if [ -n "${GITHUB_PROXY:-}" ]; then
        printf '%s/%s\n' "${GITHUB_PROXY%/}" "$1"
    else
        printf '%s\n' "$1"
    fi
}

valid_sha256() {
    [ "${#1}" -eq 64 ] || return 1
    case "$1" in *[!0-9a-fA-F]*) return 1 ;; esac
}

verify_archive() {
    valid_sha256 "$2" || return 1
    printf '%s  %s\n' "$2" "$1" | sha256sum --check --status
}

download_artifact() {
    artifact_url="$1"
    artifact_sha="$2"
    artifact_output="$3"
    if ! valid_sha256 "$artifact_sha"; then
        echo 'A pinned SHA-256 digest is required for every artifact' >&2
        return 1
    fi
    if [ -n "${RUNNER_DOWNLOAD_CACHE:-}" ]; then
        mkdir -p "$RUNNER_DOWNLOAD_CACHE"
        artifact_key=$(printf '%s' "$artifact_url" | sha256sum | cut -d ' ' -f 1)
        artifact_cached="$RUNNER_DOWNLOAD_CACHE/$artifact_key"
        if [ -f "$artifact_cached" ] && verify_archive "$artifact_cached" "$artifact_sha"; then
            cp "$artifact_cached" "$artifact_output" || return 1
            return
        fi
    fi
    curl --fail --location --retry 3 --retry-all-errors --connect-timeout 20 \
        --max-time 600 -o "$artifact_output" "$artifact_url" || return 1
    verify_archive "$artifact_output" "$artifact_sha" || return 1
    if [ -n "${RUNNER_DOWNLOAD_CACHE:-}" ]; then
        cp "$artifact_output" "$artifact_cached.$$"
        mv "$artifact_cached.$$" "$artifact_cached"
    fi
}
