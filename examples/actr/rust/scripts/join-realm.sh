#!/usr/bin/env bash
# Seed a local Realm in the actrix database and join it.
#
# Usage: join-realm.sh <actrix.db> <realm> <issuer_url> <owner_key> [client_key...]
#
# Inserts the Realm (if missing) and an operator owner invite, then
# `actr realm join` writes the owner's Realm client key to <owner_key>; the
# owner invites each client as an editor and each joins with its own key.
# Point `deployment.realm_client_key` of every actr.toml at its key file.
#
# Development only: a real deployment creates the Realm with the admin API
# (`POST /admin/api/realms`, which returns the owner invite).

set -euo pipefail

ACTR_CMD="${ACTR_CMD:-actr}"
DB="$1"
REALM="$2"
ISSUER="$3"
OWNER_KEY="$4"
shift 4

OWNER_INVITE="$(python3 - "$DB" "$REALM" <<'PY'
import base64, hashlib, json, secrets, sqlite3, sys, time

db, realm = sys.argv[1:]
raw = secrets.token_bytes(32)
secret = base64.urlsafe_b64encode(raw).rstrip(b"=").decode()
digest = hashlib.sha256(raw).hexdigest()
now = int(time.time())
conn = sqlite3.connect(db, timeout=30)
with conn:
    conn.execute(
        "INSERT OR IGNORE INTO realm (name, status, enabled, created_at) VALUES (?, 'active', 1, ?)",
        (realm, now),
    )
    conn.execute(
        "INSERT INTO realm_invite (realm, invite_id, secret_hash, role, created_by, created_at, expires_at)"
        " VALUES (?, ?, ?, 'owner', 'operator', ?, ?)",
        (realm, digest[:16], digest, now, now + 3600),
    )
invite = {"schema": "actr.realm-invite/v1", "realm": realm, "secret": secret}
text = base64.urlsafe_b64encode(json.dumps(invite).encode()).rstrip(b"=").decode()
print("ri_" + text)
PY
)"

"$ACTR_CMD" realm join "$OWNER_INVITE" --issuer "$ISSUER" --output "$OWNER_KEY" \
    --label "$(basename "$(dirname "$OWNER_KEY")")" --force >/dev/null

for CLIENT_KEY in "$@"; do
    INVITE="$("$ACTR_CMD" realm invite --key "$OWNER_KEY" --role editor | python3 -c 'import json, sys; print(json.load(sys.stdin)["invite"])')"
    "$ACTR_CMD" realm join "$INVITE" --issuer "$ISSUER" --output "$CLIENT_KEY" \
        --label "$(basename "$(dirname "$CLIENT_KEY")")" --force >/dev/null
done
