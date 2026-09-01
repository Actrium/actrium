#!/usr/bin/env bash
# Anti-regression guard for OTel payload leakage (issue #484).
#
# OpenTelemetryTracingBridge exports every tracing log event that passes the
# filter as an OTLP log record to vector → VictoriaLogs, whose query port is
# unauthenticated in the standard deployment. Formatting message payload into a
# tracing macro therefore ships user data to an effectively public backend.
#
# At the default filter_level = "info", info! and error! events ALWAYS pass the
# filter — they are strictly more dangerous than debug!/trace! (which only leak
# at non-default filters). All five log levels are therefore in scope:
# excluding info!/error! would let a future tracing::info!("{:?}", envelope)
# ship user data at the default config, which is exactly the harm this guard
# exists to prevent.
#
# This guard flags tracing macros that format payload-bearing variables. It is
# intentionally narrow to keep false positives low: only crates/actr-hyper, only the
# payload-flavored identifiers listed below, only value-formatted usages.
set -euo pipefail

if ! [ -d crates/actr-hyper ]; then
  echo "check-payload-logging: crates/actr-hyper not found, skipping (wrong workdir?)"
  exit 0
fi

# Variables treated as payload (issue #484 scope: identifiers such as actr_id /
# actor_id / realm_id and network metadata are intentionally NOT included).
PAYLOAD_VARS='envelope|payload|message|data|body'

# The macro-name prefix is `tracing::` OR bare: several crates/actr-hyper modules
# import the macros via `use tracing::{debug, info, warn, ...}` and call them
# unqualified (`info!(...)`), so anchoring on the FQN alone leaves ~45 call
# sites in security-sensitive files (ais_client.rs, storage/db.rs, ...) outside
# the guard's view. This is safe because crates/actr-hyper does NOT pull in the `log`
# crate's macros, so a bare `info!(...)` here can only resolve to `tracing`. If
# that assumption ever changes (a `use log::{...}` lands in crates/actr-hyper), this
# guard must be revisited — it could start producing false positives.
MACRO_PREFIX='(?:tracing::)?'

# The PATTERN uses Perl regex (`git grep -P`): POSIX ERE (`git grep -E`) does
# not support `\b`, `\s`, or `(?:...)`, and a prior revision relied on all
# three — so branches (2) and (3) silently never matched. -P is supported by
# the git builds on GitHub Actions runners, Homebrew, and Apple Git. The probe
# below fails loudly if it is ever unavailable, so the guard can never regress
# back into a silent no-op.
PATTERN="${MACRO_PREFIX}(debug|trace|info|warn|error)!\(.*(\{($PAYLOAD_VARS)(:[#?]+)?\}|\{:#?\?\}.*,\s*($PAYLOAD_VARS)\s*[,)]|\b($PAYLOAD_VARS)\s*=\s*[?%])"

# Fail loudly if -P is not compiled in. A silent skip would recreate the exact
# "guard never actually ran" failure mode this script exists to prevent.
# git grep exit codes: 0 = match, 1 = no match, 2/128 = usage/fatal error.
set +e
git grep -nP 'PROBE_PAYLOAD_GUARD_NEVER_MATCH' -- 'crates/actr-hyper/' >/dev/null 2>&1
probe_rc=$?
set -e
if [ "$probe_rc" -ne 0 ] && [ "$probe_rc" -ne 1 ]; then
  echo "::error::git grep -P is unavailable (rc=$probe_rc); the payload-logging guard cannot run."
  echo "Install a git built with libpcre2 (the default on GitHub Actions / Homebrew / Apple Git)."
  exit 1
fi

set +e
matches=$(git grep -nP "$PATTERN" -- 'crates/actr-hyper/**/*.rs' || true)
set -e

# Honor the per-line opt-out marker `// allow-payload-logging`. It is meant for
# the narrow case where the logged string is *guest-authored* (e.g. a WASM
# guest calling the host `log` import with a string it chose), which is a
# separate guest-trust question rather than host payload leakage — see the
# issue #484 follow-up. Every suppression must cite its rationale inline.
if [ -n "$matches" ]; then
  matches=$(printf '%s\n' "$matches" | grep -v 'allow-payload-logging' || true)
fi

if [ -n "$matches" ]; then
  echo "::error::Payload-bearing variables are formatted in tracing macros within crates/actr-hyper."
  echo "These macros bridge to OTLP log records (VictoriaLogs) when the feature is enabled."
  echo "Log only non-payload metadata (length, type, envelope_id/reply_for). See issue #484."
  echo
  echo "$matches"
  exit 1
fi

echo "check-payload-logging: no payload formatting in crates/actr-hyper tracing macros."
