# actr.toml - EchoApp linked runtime configuration (Windows, actr-cabi)
#
# EchoApp uses linked mode through the actr-cabi workload and does not use
# `actr build`. This template is rendered by run.sh with the per-run realm
# and ports, then read directly as runtime configuration by the C# host's
# ActrCabiNode.FromConfig.
#
# The actor type is declared in manifest.toml (actrium:EchoApp:0.1.0).

[signaling]
url = "ws://__HOST__:__HTTP_PORT__/signaling/ws"

[ais_endpoint]
url = "http://__HOST__:__HTTP_PORT__/ais"

[deployment]
realm_id = __REALM_ID__
realm_secret = "__REALM_SECRET__"

[discovery]
visible = true

[observability]
filter_level = "info"
tracing_enabled = false
tracing_endpoint = "http://localhost:4317"
tracing_service_name = "echo-app-winui"

[webrtc]
force_relay = false
stun_urls = ["stun:__HOST__:__ICE_PORT__"]
turn_urls = ["turn:__HOST__:__ICE_PORT__"]

[acl]

[[acl.rules]]
permission = "allow"
type = "actrium:EchoService:1.0.0"
