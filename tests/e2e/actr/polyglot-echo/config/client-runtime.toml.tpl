edition = 1

[signaling]
url = "ws://127.0.0.1:__HTTP_PORT__/signaling/ws"

[issuer_endpoint]
# Mount Issuer at `/issuer/*`; see server-runtime.toml.tpl for the rationale.
url = "http://127.0.0.1:__HTTP_PORT__/issuer"

[deployment]
realm = "__REALM__"

[discovery]
visible = true

[observability]
filter_level = "info"
tracing_enabled = false

[webrtc]
force_relay = false
stun_urls = ["stun:127.0.0.1:__ICE_PORT__"]
turn_urls = ["turn:127.0.0.1:__ICE_PORT__"]

[acl]

# Allow inbound RPCs from the EchoService at any version we publish in
# this run. Matches the publisher used by setup.sh (defaults to
# `polyglot`).
[[acl.rules]]
permission = "allow"
type = "__SERVICE_TYPE__"

[[trust]]
kind = "static"
pubkey_b64 = "__PUBLISHER_PUBKEY__"
