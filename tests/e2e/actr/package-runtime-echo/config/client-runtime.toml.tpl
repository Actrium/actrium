edition = 1

[signaling]
url = "ws://127.0.0.1:__HTTP_PORT__/signaling/ws"

[issuer_endpoint]
url = "http://127.0.0.1:__HTTP_PORT__/issuer"

[deployment]
realm = "__REALM__"
realm_client_key = "__REALM_CLIENT_KEY__"

[discovery]
visible = true

[observability]
filter_level = "info"
tracing_enabled = false
tracing_endpoint = "http://localhost:4317"
tracing_service_name = "package-runtime-echo-client"

[webrtc]
force_relay = false
stun_urls = ["stun:127.0.0.1:__ICE_PORT__"]
turn_urls = ["turn:127.0.0.1:__ICE_PORT__"]

[acl]

[[acl.rules]]
permission = "allow"
type = "e2e.actrium.example/EchoService@__ECHO_SERVICE_VERSION__"
