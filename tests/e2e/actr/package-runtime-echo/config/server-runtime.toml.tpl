edition = 1

[package]
path = "__PACKAGE_PATH__"

[signaling]
url = "ws://127.0.0.1:8081/signaling/ws"

[issuer_endpoint]
url = "http://127.0.0.1:8081/issuer"

[deployment]
realm_id = __REALM_ID__
realm_secret = "__REALM_SECRET__"

[[trust]]
# RegistryTrust fetches publisher pubkeys at {endpoint}/publisher-registry/{name}/verifying_key,
# which lives at the actrix base (PublisherRegistryService mounts /publisher), NOT under /issuer.
# [issuer_endpoint] above keeps the /issuer suffix because /register is mounted there.
kind = "registry"
endpoint = "http://127.0.0.1:8081"

[discovery]
visible = true

[observability]
filter_level = "info"
tracing_enabled = false
tracing_endpoint = "http://localhost:4317"
tracing_service_name = "package-runtime-echo-server"

[webrtc]
force_relay = false
stun_urls = ["stun:127.0.0.1:3478"]
turn_urls = ["turn:127.0.0.1:3478"]

[acl]

[[acl.rules]]
permission = "allow"
type = "e2e-actrium-local:pkg-runtime-echo-client-guest:0.1.0"
