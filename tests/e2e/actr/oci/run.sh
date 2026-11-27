#!/usr/bin/env bash
# Provision temporary, bounded volumes and run the OCI test as a non-root user.
set -euo pipefail

source_layout="$(realpath "${ACTR_OCI_TEST_LAYOUT:?set ACTR_OCI_TEST_LAYOUT}")"
test_binary="$(realpath "${ACTR_OCI_TEST_BIN:?set ACTR_OCI_TEST_BIN}")"
syd_source="$(realpath "${ACTR_OCI_TEST_SYD:?set ACTR_OCI_TEST_SYD}")"
umoci="$(realpath "${ACTR_OCI_TEST_UMOCI:-$(command -v umoci)}")"
bwrap="$(realpath "${ACTR_OCI_TEST_BWRAP:-$(command -v bwrap)}")"
test_user="${ACTR_OCI_TEST_USER:-$(id -un)}"
uid="$(id -u "$test_user")"
gid="$(id -g "$test_user")"
test "$uid" -ne 0

work="$(mktemp -d /var/tmp/actr-oci-e2e.XXXXXXXX)"
unit="actr-oci-${work##*.}"
profile="$work/apparmor"
profile_loaded=0
mounted=0
data_mounted=0
cleanup() {
    local status=$?
    if (( status != 0 )); then
        sudo find "$work/runtime" -mindepth 2 -maxdepth 2 -name guest.stderr -type f             -exec sh -c 'printf "OCI Guest diagnostic: "; head -c 4096 "$1"; echo' sh {} \;
    fi
    sudo systemctl stop "$unit.service" >/dev/null 2>&1 || true
    if (( profile_loaded )); then
        sudo apparmor_parser -R "$profile"
    fi
    if (( data_mounted )); then
        sudo umount "$work/data"
    fi
    if (( mounted )); then
        sudo umount "$work/images"
    fi
    sudo rm -rf -- "$work"
}
trap cleanup EXIT

chmod 755 "$work"
mkdir "$work/images" "$work/data"
install -m 755 "$syd_source" "$work/syd-oci"
sudo install -d -o "$uid" -g "$gid" -m 700 "$work/runtime" "$work/settings"
sudo mount -t tmpfs -o "size=256m,nr_inodes=16384,uid=$uid,gid=$gid,mode=0700,nodev,nosuid" tmpfs "$work/images"
mounted=1
sudo mount -t tmpfs -o "size=16m,nr_inodes=1024,uid=$uid,gid=$gid,mode=0700,nodev,nosuid,noexec" tmpfs "$work/data"
data_mounted=1

if [[ "$(cat /proc/sys/kernel/apparmor_restrict_unprivileged_userns 2>/dev/null || true)" == 1 ]]; then
    printf 'abi <abi/4.0>,\nprofile %s %s flags=(unconfined) {\n  userns,\n}\n' \
        "$unit" "$work/syd-oci" > "$profile"
    sudo apparmor_parser -a "$profile"
    profile_loaded=1
fi

cat > "$work/policy.syd-3" <<'POLICY'
# Fixture policy. The deployment supplies its own reviewed application policy.
sandbox/read,write,exec,net:on
allow/stat+/**
allow/fs+ext4
allow/fs+proc
allow/fs+tmpfs
sandbox/lock:on
allow/lock/read,exec+/app
allow/lock/read+/proc
allow/lock/read+/dev
allow/lock/read+/run/actr
allow/lock/connect+/run/actr/guest.sock
allow/lock/read,write,create,truncate+/data
default/read,write,exec,net:deny
allow/read+/proc/**
allow/read+/dev/**
allow/read+/app/echo
allow/read,write,create,truncate+/data/**
allow/exec+/app/echo
allow/create+!eventpoll
allow/create+!eventfd
allow/net/bind+!unnamed
allow/net/connect+/run/actr/guest.sock
lock:on
POLICY

sudo systemd-run --expand-environment=no --unit="$unit" --uid="$test_user" --wait --pipe --quiet \
    -p 'Delegate=cpu memory pids' -p DelegateSubgroup=supervisor \
    -p KillMode=control-group -p TimeoutStopSec=5s -p SendSIGKILL=yes \
    env WORK="$work" UMOCI="$umoci" BWRAP="$bwrap" TEST_BINARY="$test_binary" \
        ACTR_OCI_TEST_LAYOUT="$source_layout" ACTR_OCI_TEST_HTTPS_URL="${ACTR_OCI_TEST_HTTPS_URL:-}" \
        ACTR_OCI_BENCH_DYLIB="${ACTR_OCI_BENCH_DYLIB:-}" \
        ACTR_OCI_TEST_CALLS="${ACTR_OCI_TEST_CALLS:-3}" \
        ACTR_OCI_TEST_RESTARTS="${ACTR_OCI_TEST_RESTARTS:-1}" \
        sh -eu -c '
            group=$(sed -n "s/^0:://p" /proc/self/cgroup)
            parent=/sys/fs/cgroup/${group#/}
            parent=${parent%/supervisor}
            echo "+cpu +memory +pids" > "$parent/cgroup.subtree_control"
            export ACTR_OCI_TEST_SETTINGS="$WORK/settings/oci.toml"
            cat > "$ACTR_OCI_TEST_SETTINGS" <<EOF
syd-oci = "$WORK/syd-oci"
umoci = "$UMOCI"
bubblewrap = "$BWRAP"
supervisor = "/unused"
state-dir = "$WORK/runtime"
image-dir = "$WORK/images"
policy = "$WORK/policy.syd-3"
cgroup-parent = "$parent"
package-sha256 = ""
max-unpack-bytes = 268435456
max-unpack-inodes = 16384

[data]
path = "$WORK/data"
access = "read-write"
max-bytes = 16777216
max-inodes = 1024
EOF
            exec "$TEST_BINARY" --ignored --nocapture --test-threads=1
        '
