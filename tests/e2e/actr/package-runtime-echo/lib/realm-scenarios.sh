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
    require_cmd openssl
    export ACTR_REALM_E2E_DOCUMENTS="$RUN_DIR/realm-documents.json"
    printf '{}\n' >"$ACTR_REALM_E2E_DOCUMENTS"
    REALM_DOMAIN=localhost
    if [ "$REALM_MODE" = external ]; then REALM_DOMAIN=tenant.realm-e2e.test; fi
    REALM="echo-${RUN_ID}@${REALM_DOMAIN}"
    # Authority and its signing key must be enabled; a development Realm
    # without a Profile is deliberately not this scenario.
    PYTHONPATH="$SCRIPT_DIR/fixtures" python3 - "$LORAN_CONFIG_PATH" "${REALM_ICE_IP:-}" "$RUN_DIR/loran-tls" <<'PY'
import ipaddress, json, pathlib, re, socket, ssl, sys
from https_documents import certificates
path = pathlib.Path(sys.argv[1])
ice_ip = sys.argv[2]
tls_dir = pathlib.Path(sys.argv[3])
ca, cert, key = certificates(tls_dir)
# Preserve the system roots for later Cargo builds while adding only this
# run's disposable CA to the CLI and Host's native certificate store.
roots = ssl.create_default_context().get_ca_certs(binary_form=True)
(tls_dir / 'trusted.pem').write_text(
    ''.join(ssl.DER_cert_to_PEM_cert(root) for root in roots) + ca.read_text())
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
text = text.replace('[bind.http]\n', '[bind.http]\ncert = ' + json.dumps(str(cert))
                    + '\nkey = ' + json.dumps(str(key)) + '\n')
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
    export SSL_CERT_FILE="$RUN_DIR/loran-tls/trusted.pem"
    export CURL_CA_BUNDLE="$SSL_CERT_FILE"
    HTTP_BASE="https://127.0.0.1:${HTTP_PORT}"
    WS_BASE="wss://127.0.0.1:${HTTP_PORT}"
    section "Building the test-only CLI document transport"
    CARGO_TARGET_DIR="$ACTR_TARGET_DIR" cargo test -p actr-cli --lib --no-run \
        --message-format=json >"$RUN_DIR/cli-fixture-build.jsonl"
    REALM_CLI_FIXTURE="$(jq -r 'select(.reason == "compiler-artifact" and .target.name == "actr_cli" and .profile.test == true and .executable != null) | .executable' "$RUN_DIR/cli-fixture-build.jsonl" | tail -1)"
    [ -x "$REALM_CLI_FIXTURE" ] || fail "CLI test fixture was not built"
    if [ "${REALM_HTTPS:-0}" = 1 ]; then
        require_cmd openssl
        export ACTR_REALM_E2E_HTTPS_CONFIG="$RUN_DIR/https-ready.json"
        printf '{}\n' >"$RUN_DIR/https-artifacts.json"
        python3 "$SCRIPT_DIR/fixtures/https_documents.py" --directory "$RUN_DIR/https" \
            --documents "$ACTR_REALM_E2E_DOCUMENTS" --artifacts "$RUN_DIR/https-artifacts.json" \
            --ready "$ACTR_REALM_E2E_HTTPS_CONFIG" >"$LOG_DIR/https.log" 2>&1 &
        HTTPS_PID=$!
        local attempt=0
        while [ ! -s "$ACTR_REALM_E2E_HTTPS_CONFIG" ]; do
            kill -0 "$HTTPS_PID" 2>/dev/null || { cat "$LOG_DIR/https.log"; fail "HTTPS fixture exited"; }
            attempt=$((attempt + 1))
            [ "$attempt" -lt 100 ] || fail "HTTPS fixture did not become ready"
            sleep 0.1
        done
    fi
    if [[ "${REALM_FRONT_RELEASE:-0}" == 1 || "${REALM_SERVICE_SWITCH:-0}" == 1 ]]; then
        # Use the same selection verifier against the fixture's HTTPS transport.
        export REALM_CLI_FIXTURE LORAN_PACKAGE_VERIFIER="$RUN_DIR/package-verifier"
        cat >"$LORAN_PACKAGE_VERIFIER" <<'VERIFIER'
#!/usr/bin/env bash
set -euo pipefail
[[ "$#" == 8 && "$1" == realm && "$2" == check-selection &&
   "$3" == --selection && "$5" == --output && "$7" == --trust-dir ]]
mkdir -p "$6"
ACTR_REALM_E2E_VERIFY_SELECTION="$4" ACTR_REALM_E2E_VERIFY_OUTPUT="$6" \
    ACTR_REALM_E2E_VERIFY_TRUST="$8" "$REALM_CLI_FIXTURE" --ignored --nocapture \
    --exact commands::realm_packages::tests::front_e2e::realm_package_verifier_fixture \
    >"$6/fixture.log" 2>&1 || { cat "$6/fixture.log" >&2; exit 1; }
cat "$6/report.json"
VERIFIER
        chmod 700 "$LORAN_PACKAGE_VERIFIER"
    fi
}

post_realm_management() {
    realm_curl --fail-with-body -sS -X POST "${HTTP_BASE}/actr/realm-management" \
        -H 'Content-Type: application/json' --data-binary "@$1" >"$2"
}

realm_fixture_join() {
    local label="$1" invite_file="$2" key_file="$3" result_file="$4"
    local request="$RUN_DIR/join-${label}-request.json"
    jq -n --arg invite "$invite_file" --arg key "$key_file" --arg client_label "$label" \
        --arg result "$result_file" --arg package "$RUN_DIR/downloads/${label}.actr" \
        --argjson https "${REALM_HTTPS:-0}" --arg backend "$BACKEND" \
        '{invite_file:$invite,key_file:$key,"label":$client_label,result_file:$result}
         + (if $https == 1 then {package_file:$package,verify_pair:($backend=="v2-component")} else {} end)' >"$request" || return
    ACTR_REALM_E2E_REQUEST="$request" "$REALM_CLI_FIXTURE" \
        --exact commands::realm_members::tests::realm_runtime_join_fixture \
        --ignored --nocapture >"$LOG_DIR/join-${label}.log" 2>&1
}

# A real, signed Keel Front for the Realm's second selection.
build_realm_front() {
    local version="${1:-1.0.0}" output="${2:-$DIST_DIR/realm-front.actr}"
    local service_type="${3:-$PUBLISHER/EchoService@$SERVICE_VERSION}"
    local payload="$RUN_DIR/front-payload"
    mkdir -p "$payload/workload" "$payload/contracts"
    printf 'export default {};\n' >"$payload/workload/guest.js"
    cp "$SCRIPT_DIR/proto/echo.proto" "$payload/contracts/echo.proto"
    cat >"$payload/manifest.toml" <<EOF
format = "actr-package/2"
[actor]
publisher = "$PUBLISHER"
name = "EchoFront"
version = "$version"
[workload]
profile = "keel.guest/1"
entry = "workload/guest.js"
[workload.keel]
runtime = "js"
guest-api = "keel.guest/1"
[host]
profile = "actr.host/1"
api = "^1.0.0"
[dependencies.service]
actor = "${service_type%@*}"
version = "=${service_type##*@}"
contract = "contracts/echo.proto"
required = true
[signature]
algorithm = "ed25519"
key-id = "computed-at-pack-time"
[[files]]
path = "workload/guest.js"
role = "workload"
media-type = "text/javascript"
size = 0
sha256 = "0000000000000000000000000000000000000000000000000000000000000000"
[[files]]
path = "contracts/echo.proto"
role = "contract"
media-type = "text/plain"
size = 0
sha256 = "0000000000000000000000000000000000000000000000000000000000000000"
EOF
    run_actr pkg pack --manifest-path "$payload/manifest.toml" --payload-root "$payload" \
        --key "$SERVICE_KEYCHAIN" --output "$output"
}

fixture_service_actor() {
    local serial
    serial="$(sqlite3 "$LORAN_DB" "SELECT printf('%x', serial_number)
        FROM issued_actor_identity WHERE realm = '$REALM'
          AND actor_type = '$1' AND auth_mode = 1;")"
    [[ -n "$serial" && "$serial" != *$'\n'* ]] || fail "expected one registered Service Actor"
    printf '%s.%s\n' "$serial" "$REALM"
}

start_fixture_candidate() {
    local SERVICE_PACKAGE="$1" SERVER_RUNTIME_PATH="$RUN_DIR/server-next.toml"
    local original_pid="$SERVER_PID" home_dir hyper_dir
    IFS=$'\t' read -r home_dir hyper_dir <<<"$(mk_isolated_env service-next)"
    start_server_isolated "$home_dir" "$hyper_dir" "server-next.log"
    CANDIDATE_SERVICE_PID="$SERVER_PID"
    SERVER_PID="$original_pid"
    expect_server_started "server-next.log" "$CANDIDATE_SERVICE_PID"
}

run_package_release_roundtrips() {
    local candidate="$DIST_DIR/realm-front-next.actr" selection="$RUN_DIR/front-next-selection.json"
    local request="$RUN_DIR/front-release-request.json" front_sha front_href package_key
    local original_service_type="$PUBLISHER/EchoService@$SERVICE_VERSION"
    local target_service_type="$original_service_type" actors=null
    local next_service="" service_sha="" service_href=""
    if [ "${REALM_SERVICE_SWITCH:-0}" = 1 ]; then
        section "Joint Front and Service identity switch and rollback"
        target_service_type="$PUBLISHER/EchoServiceNext@$SERVICE_VERSION"
        next_service="$DIST_DIR/echo-v2-next.actr"
        local payload="$RUN_DIR/v2-echo-next-payload"
        cp -R "$RUN_DIR/v2-echo-payload" "$payload"
        sed 's/^name = "EchoService"$/name = "EchoServiceNext"/' \
            "$RUN_DIR/v2-echo-payload/manifest.toml" >"$payload/manifest.toml"
        run_actr pkg pack --manifest-path "$payload/manifest.toml" --payload-root "$payload" \
            --key "$SERVICE_KEYCHAIN" --output "$next_service"
        service_sha="$(shasum -a 256 "$next_service" | cut -d' ' -f1)"
        service_href="https://packages.realm-e2e.test/${service_sha}.actr"
        start_fixture_candidate "$next_service"
        actors="$(jq -nc --arg old "$(fixture_service_actor "$original_service_type")" \
            --arg new "$(fixture_service_actor "$target_service_type")" '[$old,$new]')"
    else
        section "Front upgrade and rollback against one running Service"
    fi
    build_realm_front "1.1.0" "$candidate" "$target_service_type"
    front_sha="$(shasum -a 256 "$candidate" | cut -d' ' -f1)"
    front_href="https://packages.realm-e2e.test/${front_sha}.actr"
    jq --arg href "$front_href" --arg sha "$front_sha" --arg type "$PUBLISHER/EchoFront@1.1.0" \
        --arg sh "$service_href" --arg ss "$service_sha" --arg st "$target_service_type" \
        '.packages[.front_package] = {href:$href, sha256:$sha, type:$type}
         | if $sh != "" then .packages[.primary_package] = {href:$sh, sha256:$ss, type:$st} else . end' \
        "$RUN_DIR/selection.json" >"$selection"
    jq --arg href "$front_href" --arg file "$candidate" --arg sh "$service_href" --arg sf "$next_service" \
        '.[$href] = $file | if $sh != "" then .[$sh] = $sf else . end' "$RUN_DIR/https-artifacts.json" \
        >"$RUN_DIR/https-front-release.json"
    mv "$RUN_DIR/https-front-release.json" "$RUN_DIR/https-artifacts.json"
    package_key="$(jq -er '.public_key' "$SERVICE_PUBLIC_KEY")"
    jq -n --arg owner "$REALM_OWNER_KEY" --arg front "$REALM_CLIENT_KEY" \
        --arg selection "$selection" --arg output "$RUN_DIR/front-release-cache" \
        --arg public "https://$REALM_DOMAIN/realms/${REALM%@*}" \
        --arg served "${HTTP_BASE}/actr/realms/$REALM_DOMAIN/${REALM%@*}" \
        --arg ca "$RUN_DIR/loran-tls/ca.pem" --arg key "$package_key" \
        --argjson actors "$actors" \
        '{owner_key:$owner,front_key:$front,selection:$selection,output:$output,
          public_profile_url:$public,issuer_profile_url:$served,issuer_ca:$ca,package_key:$key,
          service_actors:$actors}' >"$request"
    ACTR_REALM_E2E_REQUEST="$request" timeout 180 "$REALM_CLI_FIXTURE" \
        --exact commands::realm_packages::tests::front_e2e::realm_front_release_fixture \
        --ignored --nocapture >"$LOG_DIR/front-release.log" 2>&1 ||
        { cat "$LOG_DIR/front-release.log"; fail "Package release round-trips failed"; }
    kill -0 "$SERVER_PID" 2>/dev/null || fail "Original Service stopped during the release"
    if [ "${REALM_SERVICE_SWITCH:-0}" = 1 ]; then
        kill -0 "$CANDIDATE_SERVICE_PID" 2>/dev/null || fail "Candidate Service stopped during the release"
        grep -E '^SERVICE_(SWITCH|PACKAGE_GATE) ' "$LOG_DIR/front-release.log"
        success "Front reached the original, upgraded, and restored Actor in order"
    else
        grep '^FRONT_RELEASE ' "$LOG_DIR/front-release.log"
        success "All three Fronts called the same Service Actor"
    fi
}

create_profile_realm() {
    section "Creating the ${REALM_MODE} domain Realm through its formal API"
    local base="${HTTP_BASE}" create="$RUN_DIR/realm-create.json"
    local profile="$RUN_DIR/realm-profile.json" descriptor="$RUN_DIR/realm-descriptor.json"
    local package_hash package_type package_href
    package_hash="$(python3 - "$SERVICE_PACKAGE" <<'PY'
import hashlib, pathlib, sys
print(hashlib.sha256(pathlib.Path(sys.argv[1]).read_bytes()).hexdigest())
PY
)"
    package_type="${PUBLISHER}/EchoService@${SERVICE_VERSION}"
    package_href="https://packages.realm-e2e.test/${package_hash}.actr"
    build_realm_front
    local front_hash front_href selection="$RUN_DIR/selection.json"
    front_hash="$(shasum -a 256 "$DIST_DIR/realm-front.actr" | cut -d' ' -f1)"
    front_href="https://packages.realm-e2e.test/${front_hash}.actr"
    jq -n --arg href "$package_href" --arg sha "$package_hash" --arg type "$package_type" \
        --arg fh "$front_href" --arg fs "$front_hash" --arg ft "$PUBLISHER/EchoFront@1.0.0" \
        '{primary_package:"service",front_package:"front",packages:{service:{href:$href,sha256:$sha,type:$type},front:{href:$fh,sha256:$fs,type:$ft}}}' >"$selection"
    if [ "$REALM_MODE" = provider ]; then
        jq --arg realm "$REALM" '. + {realm:$realm}' "$selection" >"$RUN_DIR/create-body.json"
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
            --provider localhost --selection "$selection" >"$RUN_DIR/create-request.json"
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
        run_actr realm sign-profile --key "$RUN_DIR/domain-key.json" --selection "$selection" "$create" \
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
        '.realm.name == $realm and .packages[.primary_package].sha256 == $hash and .packages[.primary_package].type == $type and .provider.id == "localhost"' "$profile" >/dev/null
    jq -r '.owner_invite' "$create" >"$RUN_DIR/owner-invite.txt"
    REALM_OWNER_KEY="$RUN_DIR/keys/owner.json"
    REALM_CLIENT_KEY="$RUN_DIR/keys/client.json"

    if [ "${REALM_HTTPS:-0}" = 1 ]; then
        # Signing remains in the builder. Only the public descriptor is served.
        run_actr publisher profile keygen --key-id https-e2e-root \
            -o "$PUBLISHER_PRIVATE_DIR/profile-root.json" >/dev/null
        run_actr publisher profile sign -k "$PUBLISHER_PRIVATE_DIR/profile-root.json" \
            --domain "$PUBLISHER" --package-key "$SERVICE_PUBLIC_KEY" \
            --security-contact mailto:e2e@example.invalid >"$RUN_DIR/publisher-member.json"
        jq -n --arg domain "$PUBLISHER" --slurpfile publisher "$RUN_DIR/publisher-member.json" \
            '{version:1,authority:("actr://"+$domain),services:[{rel:"contracts",href:("https://"+$domain+"/contracts")}],publisher:$publisher[0]}' \
            >"$RUN_DIR/publisher-descriptor.json"
        publish_realm_document "https://$PUBLISHER/.well-known/actr" "$RUN_DIR/publisher-descriptor.json"
        jq -n --arg url "$package_href" --arg file "$SERVICE_PACKAGE" --arg front "$front_href" --arg ui "$DIST_DIR/realm-front.actr" '{($url):$file,($front):$ui}' >"$RUN_DIR/https-artifacts.json"

        openssl req -x509 -newkey rsa:2048 -nodes -days 1 -subj '/CN=Untrusted E2E CA' \
            -addext 'basicConstraints=critical,CA:TRUE' -addext 'keyUsage=critical,keyCertSign,cRLSign' \
            -keyout "$PUBLISHER_PRIVATE_DIR/untrusted-ca-key.pem" -out "$RUN_DIR/https/untrusted-ca.pem" \
            >"$LOG_DIR/untrusted-ca.log" 2>&1
        jq --arg ca "$RUN_DIR/https/untrusted-ca.pem" '.ca_file = $ca' \
            "$ACTR_REALM_E2E_HTTPS_CONFIG" >"$RUN_DIR/untrusted-https.json"
        ACTR_REALM_E2E_HTTPS_CONFIG="$RUN_DIR/untrusted-https.json" \
            expect_https_join_rejected certificate "$RUN_DIR/owner-invite.txt" 'certificate'

        # An HTTP 200 is insufficient: the real resolver must reject a
        # descriptor for another authority before consuming the owner invite.
        jq '.authority = "actr://wrong.realm-e2e.test"' "$descriptor" >"$RUN_DIR/wrong-authority.json"
        publish_realm_document "https://$REALM_DOMAIN/.well-known/actr" "$RUN_DIR/wrong-authority.json"
        expect_https_join_rejected authority "$RUN_DIR/owner-invite.txt" 'authority'
        publish_realm_document "https://$REALM_DOMAIN/.well-known/actr" "$descriptor"

        cp "$SERVICE_PACKAGE" "$RUN_DIR/corrupted.actr"
        printf 'corrupted' >>"$RUN_DIR/corrupted.actr"
        jq -n --arg url "$package_href" --arg file "$RUN_DIR/corrupted.actr" '{($url):$file}' >"$RUN_DIR/https-artifacts.json"
        expect_https_join_rejected digest "$RUN_DIR/owner-invite.txt" 'PACKAGE_DIGEST_MISMATCH'
        jq -n --arg url "$package_href" --arg file "$SERVICE_PACKAGE" --arg front "$front_href" --arg ui "$DIST_DIR/realm-front.actr" '{($url):$file,($front):$ui}' >"$RUN_DIR/https-artifacts.json"

        jq '.publisher.signature.value = "invalid"' "$RUN_DIR/publisher-descriptor.json" >"$RUN_DIR/bad-publisher.json"
        publish_realm_document "https://$PUBLISHER/.well-known/actr" "$RUN_DIR/bad-publisher.json"
        expect_https_join_rejected publisher "$RUN_DIR/owner-invite.txt" 'signature'
        publish_realm_document "https://$PUBLISHER/.well-known/actr" "$RUN_DIR/publisher-descriptor.json"

        if [ "$REALM_MODE" = external ]; then
            # The domain owner legitimately signs these higher-sequence
            # Profiles; rejection must happen at package verification.
            jq '.packages[.primary_package].type = "e2e.actrium.example/WrongService@1.0.0"' "$profile" >"$RUN_DIR/wrong-type-input.json"
            run_actr realm sign-profile --key "$RUN_DIR/domain-key.json" "$RUN_DIR/wrong-type-input.json" \
                >"$RUN_DIR/wrong-type-request.json"
            jq '.profile' "$RUN_DIR/wrong-type-request.json" >"$RUN_DIR/wrong-type-profile.json"
            publish_realm_document "https://$REALM_DOMAIN/realms/${REALM%@*}" "$RUN_DIR/wrong-type-profile.json"
            expect_https_join_rejected type "$RUN_DIR/owner-invite.txt" 'SERVICE_ACTOR_TYPE_MISMATCH'
            publish_realm_document "https://$REALM_DOMAIN/realms/${REALM%@*}" "$profile"

            python3 - "$SERVICE_PACKAGE" "$RUN_DIR/bad-signature.actr" <<'PY'
import sys, zipfile
with zipfile.ZipFile(sys.argv[1]) as source, zipfile.ZipFile(sys.argv[2], 'w', compression=zipfile.ZIP_STORED) as target:
    for name in source.namelist():
        body = source.read(name)
        if name == 'manifest.sig':
            body = bytes([body[0] ^ 1]) + body[1:]
        target.writestr(name, body)
PY
            local bad_hash
            bad_hash="$(python3 -c 'import hashlib,sys; print(hashlib.sha256(open(sys.argv[1],"rb").read()).hexdigest())' "$RUN_DIR/bad-signature.actr")"
            jq --arg hash "$bad_hash" '.packages[.primary_package].sha256 = $hash' "$profile" >"$RUN_DIR/bad-signature-input.json"
            run_actr realm sign-profile --key "$RUN_DIR/domain-key.json" "$RUN_DIR/bad-signature-input.json" \
                >"$RUN_DIR/bad-signature-request.json"
            jq '.profile' "$RUN_DIR/bad-signature-request.json" >"$RUN_DIR/bad-signature-profile.json"
            publish_realm_document "https://$REALM_DOMAIN/realms/${REALM%@*}" "$RUN_DIR/bad-signature-profile.json"
            jq -n --arg url "$package_href" --arg file "$RUN_DIR/bad-signature.actr" '{($url):$file}' >"$RUN_DIR/https-artifacts.json"
            expect_https_join_rejected signature "$RUN_DIR/owner-invite.txt" 'signature'
            publish_realm_document "https://$REALM_DOMAIN/realms/${REALM%@*}" "$profile"
            jq -n --arg url "$package_href" --arg file "$SERVICE_PACKAGE" --arg front "$front_href" --arg ui "$DIST_DIR/realm-front.actr" '{($url):$file,($front):$ui}' >"$RUN_DIR/https-artifacts.json"
        fi
    fi

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
    if [ "${REALM_HTTPS:-0}" = 1 ]; then
        # Hosts now execute bytes fetched through the Profile's HTTPS href,
        # not the builder's local package path.
        SERVICE_PACKAGE="$RUN_DIR/downloads/owner.actr"
    fi
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

expect_https_join_rejected() {
    local label="$1" invite="$2" marker="$3"
    if realm_fixture_join "$label" "$invite" "$RUN_DIR/keys/${label}.json" "$RUN_DIR/${label}-result.json"; then
        fail "HTTPS negative case was accepted: $label"
    fi
    grep -qi "$marker" "$LOG_DIR/join-${label}.log" || { cat "$LOG_DIR/join-${label}.log"; fail "Wrong rejection for $label"; }
    [ ! -e "$RUN_DIR/keys/${label}.json" ] || fail "Rejected HTTPS input created a key"
    [ ! -e "$RUN_DIR/downloads/${label}.actr" ] || fail "Rejected HTTPS input populated the package cache"
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
    local base="${HTTP_BASE}" profile="$RUN_DIR/retired-profile.json"
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
