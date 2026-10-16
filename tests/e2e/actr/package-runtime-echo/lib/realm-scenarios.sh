#!/usr/bin/env bash
# Two formal Realm creation paths sharing the package-runtime Echo scenario.
# Sourced by run.sh; all secrets and documents live in its disposable RUN_DIR.

realm_curl() {
    curl --connect-timeout 5 --max-time 30 "$@"
}

publish_realm_document() {
    python3 - "$ACTR_REALM_E2E_DOCUMENTS" "$1" "$2" <<'PY'
import json, os, sys
path, url, source = sys.argv[1:]
with open(path) as stream:
    documents = json.load(stream)
with open(source) as stream:
    documents[url] = json.load(stream)
with open(path + ".new", "w") as stream:
    json.dump(documents, stream)
os.replace(path + ".new", path)
PY
}

prepare_realm_scenario() {
    export ACTR_REALM_E2E_DOCUMENTS="$RUN_DIR/realm-documents.json"
    printf '{}\n' >"$ACTR_REALM_E2E_DOCUMENTS"
    REALM_DOMAIN=localhost
    if [ "$REALM_MODE" = external ]; then REALM_DOMAIN=tenant.realm-e2e.test; fi
    REALM="echo-${RUN_ID}@${REALM_DOMAIN}"
    # Authority and its signing key must be enabled; a development Realm
    # without a Profile is deliberately not this scenario.
    python3 - "$ACTRIX_CONFIG_PATH" "${REALM_ICE_IP:-}" <<'PY'
import ipaddress, pathlib, re, socket, sys
path = pathlib.Path(sys.argv[1])
ice_ip = sys.argv[2]
if not ice_ip:
    # UDP connect selects a local route without transmitting any packet.
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as probe:
        try:
            probe.connect(('192.0.2.1', 9))
            ice_ip = probe.getsockname()[0]
        except OSError:
            ice_ip = '127.0.0.1'
ice_ip = str(ipaddress.IPv4Address(ice_ip))
text = path.read_text().replace('[authority]\n', '[authority]\nenabled = true\n')
text = text.replace('domain_name = "127.0.0.1"', 'domain_name = "localhost"')
text = text.replace('enable = 25\n', 'enable = 31\n')
start, rest = text.split('[bind.ice]\n', 1)
ice, end = rest.split('\n[turn]', 1)
ice = re.sub(r'^ip = .*$', 'ip = "0.0.0.0"', ice, flags=re.M)
ice = re.sub(r'^advertised_ip = .*$', 'advertised_ip = "' + ice_ip + '"', ice, flags=re.M)
text = start + '[bind.ice]\n' + ice + '\n[turn]' + end
# The network transport fixture does not exercise periodic HTTPS refresh.
text += '\n[realm]\nprofile_refresh_secs = 86400\n'
path.write_text(text)
PY
    section "Building the test-only CLI document transport"
    CARGO_TARGET_DIR="$ACTR_TARGET_DIR" cargo test -p actr-cli --lib --no-run \
        --message-format=json >"$RUN_DIR/cli-fixture-build.jsonl"
    REALM_CLI_FIXTURE="$(jq -r 'select(.reason == "compiler-artifact" and .target.name == "actr_cli" and .profile.test == true and .executable != null) | .executable' "$RUN_DIR/cli-fixture-build.jsonl" | tail -1)"
    [ -x "$REALM_CLI_FIXTURE" ] || fail "CLI test fixture was not built"
}

post_realm_management() {
    realm_curl --fail-with-body -sS -X POST "http://127.0.0.1:${HTTP_PORT}/actr/realm-management" \
        -H 'Content-Type: application/json' --data-binary "@$1" >"$2"
}

realm_fixture_join() {
    local label="$1" invite_file="$2" key_file="$3" result_file="$4"
    local request="$RUN_DIR/join-${label}-request.json"
    jq -n --arg invite "$invite_file" --arg key "$key_file" --arg label "$label" \
        --arg result "$result_file" \
        '{invite_file:$invite,key_file:$key,label:$label,result_file:$result}' >"$request"
    ACTR_REALM_E2E_REQUEST="$request" "$REALM_CLI_FIXTURE" \
        --exact commands::realm_members::tests::realm_runtime_join_fixture \
        --ignored --nocapture >"$LOG_DIR/join-${label}.log" 2>&1
}

create_profile_realm() {
    section "Creating the ${REALM_MODE} domain Realm through its formal API"
    local base="http://127.0.0.1:${HTTP_PORT}" create="$RUN_DIR/realm-create.json"
    local profile="$RUN_DIR/realm-profile.json" descriptor="$RUN_DIR/realm-descriptor.json"
    local package_hash package_type package_href
    package_hash="$(python3 - "$SERVICE_PACKAGE" <<'PY'
import hashlib, pathlib, sys
print(hashlib.sha256(pathlib.Path(sys.argv[1]).read_bytes()).hexdigest())
PY
)"
    package_type="${PUBLISHER}/EchoService@${SERVICE_VERSION}"
    package_href="https://packages.realm-e2e.test/${package_hash}.actr"
    if [ "$REALM_MODE" = provider ]; then
        jq -n --arg realm "$REALM" --arg href "$package_href" --arg sha "$package_hash" \
            --arg type "$package_type" \
            '{realm:$realm,package:{href:$href,sha256:$sha,type:$type}}' >"$RUN_DIR/create-body.json"
        realm_curl --fail-with-body -sS -X POST "$base/admin/api/realms" \
            -H "Authorization: Bearer ${ADMIN_TOKEN}" -H 'Content-Type: application/json' \
            --data-binary "@$RUN_DIR/create-body.json" >"$create"
        jq -e '.realm.status == "active" and (.owner_invite | startswith("ri_"))' "$create" >/dev/null
        realm_curl -fsS "$base/actr/realms/$REALM_DOMAIN/${REALM%@*}" >"$profile"
        realm_curl -fsS "$base/.well-known/actr" >"$descriptor"
        # Public document transport maps HTTPS virtual URLs to these bytes.
        # The real Provider-generated, self-signed keys are kept unchanged.
        jq '.services = [{rel:"realms",href:"https://localhost/realms",protocol:"actr-realm-profile/1"}]' \
            "$descriptor" >"$descriptor.new"
        mv "$descriptor.new" "$descriptor"
    else
        run_actr realm keygen --key-id realm-e2e --output "$RUN_DIR/domain-key.json" \
            >"$RUN_DIR/domain-keys.json"
        jq -n --slurpfile keys "$RUN_DIR/domain-keys.json" \
            '{version:1,authority:"actr://tenant.realm-e2e.test",realm_signing_keys:$keys[0],services:[{rel:"realms",href:"https://tenant.realm-e2e.test/realms",protocol:"actr-realm-profile/1"}]}' >"$descriptor"
        publish_realm_document "https://$REALM_DOMAIN/.well-known/actr" "$descriptor"
        run_actr realm create-request --key "$RUN_DIR/domain-key.json" --realm "$REALM" \
            --provider localhost --package-href "$package_href" --package-sha256 "$package_hash" \
            --package-type "$package_type" >"$RUN_DIR/create-request.json"
        jq '.nonce += "-tampered"' "$RUN_DIR/create-request.json" >"$RUN_DIR/bad-signature-request.json"
        local status
        status="$(realm_curl -sS -o "$RUN_DIR/bad-signature.json" -w '%{http_code}' -X POST \
            "$base/actr/realm-management" -H 'Content-Type: application/json' \
            --data-binary "@$RUN_DIR/bad-signature-request.json")"
        [ "$status" = 403 ] || fail "Invalid management signature was accepted: $status"
        post_realm_management "$RUN_DIR/create-request.json" "$create"
        jq -e '.status == "pending" and .owner_invite == null' "$create" >/dev/null
        # A replay must fail before any new Realm or invitation is created.
        status="$(realm_curl -sS -o "$RUN_DIR/replay.json" -w '%{http_code}' -X POST \
            "$base/actr/realm-management" -H 'Content-Type: application/json' \
            --data-binary "@$RUN_DIR/create-request.json")"
        [ "$status" = 400 ] || fail "Replayed creation was accepted: $status"
        run_actr realm sign-profile --key "$RUN_DIR/domain-key.json" "$create" \
            >"$RUN_DIR/publish-request.json"
        jq '.profile' "$RUN_DIR/publish-request.json" >"$profile"
        publish_realm_document "https://$REALM_DOMAIN/realms/${REALM%@*}" "$profile"
        post_realm_management "$RUN_DIR/publish-request.json" "$RUN_DIR/activated.json"
        jq -e '.status == "active" and (.owner_invite | startswith("ri_"))' "$RUN_DIR/activated.json" >/dev/null
        cp "$RUN_DIR/activated.json" "$create"
    fi
    publish_realm_document "https://$REALM_DOMAIN/.well-known/actr" "$descriptor"
    publish_realm_document "https://$REALM_DOMAIN/realms/${REALM%@*}" "$profile"
    jq -e --arg realm "$REALM" --arg hash "$package_hash" --arg type "$package_type" \
        '.realm.name == $realm and .package.sha256 == $hash and .package.type == $type and .provider.id == "localhost"' "$profile" >/dev/null
    jq -r '.owner_invite' "$create" >"$RUN_DIR/owner-invite.txt"
    REALM_OWNER_KEY="$RUN_DIR/keys/owner.json"
    REALM_CLIENT_KEY="$RUN_DIR/keys/client.json"

    # A tampered Profile must fail verification before redeeming the invite
    # or creating a client key. Neither signing nor membership is mocked.
    jq '.realm.display_name = "tampered"' "$profile" >"$RUN_DIR/tampered-profile.json"
    publish_realm_document "https://$REALM_DOMAIN/realms/${REALM%@*}" "$RUN_DIR/tampered-profile.json"
    if realm_fixture_join tampered "$RUN_DIR/owner-invite.txt" "$RUN_DIR/keys/tampered.json" "$RUN_DIR/tampered-result.json"; then
        fail "Tampered Realm Profile was accepted"
    fi
    [ ! -e "$RUN_DIR/keys/tampered.json" ] || fail "Tampered Profile created a key"
    publish_realm_document "https://$REALM_DOMAIN/realms/${REALM%@*}" "$profile"

    realm_fixture_join owner "$RUN_DIR/owner-invite.txt" "$REALM_OWNER_KEY" "$RUN_DIR/owner-result.json" ||
        { cat "$LOG_DIR/join-owner.log"; fail "Owner join failed"; }
    jq -e '.joined.role == "owner"' "$RUN_DIR/owner-result.json" >/dev/null
    if realm_fixture_join replay "$RUN_DIR/owner-invite.txt" "$RUN_DIR/keys/replay.json" "$RUN_DIR/replay-result.json"; then
        fail "Owner invitation was redeemed twice"
    fi
    grep -q 'invite_used' "$LOG_DIR/join-replay.log" || fail "Invite replay did not reach membership validation"
    [ ! -e "$RUN_DIR/keys/replay.json" ] || fail "Failed invite replay retained a new key"
    run_actr realm invite --key "$REALM_OWNER_KEY" --role editor >"$RUN_DIR/client-invite.json"
    jq -r '.invite' "$RUN_DIR/client-invite.json" >"$RUN_DIR/client-invite.txt"
    realm_fixture_join client "$RUN_DIR/client-invite.txt" "$REALM_CLIENT_KEY" "$RUN_DIR/client-result.json" ||
        { cat "$LOG_DIR/join-client.log"; fail "Client join failed"; }
    jq -e '.joined.role == "editor"' "$RUN_DIR/client-result.json" >/dev/null
    REALM_CLIENT_MEMBER_ID="$(jq -r '.joined.member_id' "$RUN_DIR/client-result.json")"
    jq '.profile' "$RUN_DIR/owner-result.json" >"$RUN_DIR/verified-profile.json"
    python3 - "$REALM_OWNER_KEY" "$REALM_CLIENT_KEY" "$REALM" <<'PY'
import json, sys
owner, client = [json.load(open(path)) for path in sys.argv[1:3]]
assert owner['realm'] == client['realm'] == sys.argv[3]
assert owner['provider_id'] == client['provider_id'] == 'localhost'
assert owner['key_id'] != client['key_id']
PY
    success "Signed Profile discovery, owner/editor joins and negative cases passed"
}

# The E2E orchestrator translates the verified Profile into the runtime's
# existing config API. This does not add automatic Profile resolution to Host.
apply_verified_realm_config() {
    [ "$REALM_MODE" != legacy ] || return 0
    python3 - "$RUN_DIR/verified-profile.json" "$1" <<'PY'
import json, pathlib, re, sys
profile = json.load(open(sys.argv[1]))
path = pathlib.Path(sys.argv[2])
text = path.read_text()
# Relay-only keeps this Realm test focused on authenticated infrastructure
# rather than multicast host-candidate discovery on the developer's network.
text = text.replace('force_relay = false', 'force_relay = true')
for section, value in [('signaling', profile['provider']['signaling'][0]['url']),
                       ('issuer_endpoint', profile['issuer']['url'])]:
    text, count = re.subn(r'(\[' + section + r'\]\nurl = )[^\n]+',
                         lambda m: m[1] + json.dumps(value), text)
    assert count == 1, section
for kind in ['stun', 'turn']:
    urls = [url for endpoint in profile['provider']['ice'] for url in endpoint['urls']
            if url.startswith((kind + ':', kind + 's:'))]
    text, count = re.subn(kind + r'_urls = [^\n]+',
                         lambda m: kind + '_urls = ' + json.dumps(urls), text)
    assert count == 1, kind
path.write_text(text)
PY
}

retire_profile_realm() {
    local base="http://127.0.0.1:${HTTP_PORT}" profile="$RUN_DIR/retired-profile.json"
    if [ "$REALM_MODE" = provider ]; then
        realm_curl --fail-with-body -sS -X POST "$base/admin/api/realms/${REALM}/retire" \
            -H "Authorization: Bearer ${ADMIN_TOKEN}" >"$RUN_DIR/retired.json"
        realm_curl -fsS "$base/actr/realms/$REALM_DOMAIN/${REALM%@*}" >"$profile"
    else
        run_actr realm retire --key "$RUN_DIR/domain-key.json" "$RUN_DIR/realm-profile.json" \
            >"$RUN_DIR/retire-request.json"
        post_realm_management "$RUN_DIR/retire-request.json" "$RUN_DIR/retired.json"
        jq '.profile' "$RUN_DIR/retire-request.json" >"$profile"
    fi
    jq -e '.realm.status == "retired" and .sequence > 1' "$profile" >/dev/null
    publish_realm_document "https://$REALM_DOMAIN/realms/${REALM%@*}" "$profile"
    if realm_fixture_join retired "$RUN_DIR/owner-invite.txt" "$RUN_DIR/keys/retired.json" "$RUN_DIR/retired-result.json"; then
        fail "Retired Realm was accepted by the Profile resolver"
    fi
    grep -q 'is retired' "$LOG_DIR/join-retired.log" || fail "Join did not reject the retired Profile"
    [ ! -e "$RUN_DIR/keys/retired.json" ] || fail "Retired Realm created a client key"
    success "Realm retirement was observed by the client resolver"
}
