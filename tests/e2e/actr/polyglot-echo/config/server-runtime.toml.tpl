edition = 1

[package]
path = "__PACKAGE_PATH__"

[signaling]
url = "ws://127.0.0.1:__HTTP_PORT__/signaling/ws"

[issuer_endpoint]
# Mount Issuer at the `/issuer/*` prefix; the bare-root form parses with a
# trailing slash and yields `http://host//register` once Hyper formats
# `{endpoint}/register`. mock-actrix exposes the same routes under both
# `/` and `/issuer/*`, so pinning the prefix here is safe.
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

# Each polyglot driver registers under its own actr_type via
# `Node::from_config_with_package` + a per-driver `manifest.toml`,
# so the server's allow-list enumerates them explicitly. Adding a new
# language driver means appending another rule here, not loosening the
# allow-list to a wildcard.
[[acl.rules]]
permission = "allow"
type = "polyglot/RustDriver@0.1.0"

[[acl.rules]]
permission = "allow"
type = "polyglot/TsDriver@0.1.0"

[[trust]]
kind = "static"
pubkey_b64 = "__PUBLISHER_PUBKEY__"
