edition = 1

[package]
path = "__PACKAGE_PATH__"

[signaling]
url = "ws://127.0.0.1:__HTTP_PORT__/signaling/ws"

[issuer_endpoint]
url = "http://127.0.0.1:__HTTP_PORT__/issuer"

[deployment]
realm = "__REALM__"
realm_client_key = "__REALM_CLIENT_KEY__"

[[trust]]
# RegistryTrust fetches publisher pubkeys at {endpoint}/registry/publishers/{name}/verifying-key,
# which lives at the loran base, not under /issuer.
# [issuer_endpoint] above keeps the /issuer suffix because /register is mounted there.
kind = "registry"
endpoint = "__HTTP_BASE__"

[discovery]
visible = true

[observability]
filter_level = "info"
tracing_enabled = false
tracing_endpoint = "http://localhost:4317"
tracing_service_name = "package-runtime-echo-server"

[webrtc]
force_relay = false
stun_urls = ["stun:127.0.0.1:__ICE_PORT__"]
turn_urls = ["turn:127.0.0.1:__ICE_PORT__"]

[acl]

[[acl.rules]]
permission = "allow"
type = "e2e-actrium-local/pkg-runtime-echo-client-guest@0.1.0"
