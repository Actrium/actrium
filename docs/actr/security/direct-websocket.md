# Migration from direct WebSocket listeners

Application-side peer WebSocket listeners have been replaced by outbound,
end-to-end encrypted connections to the Loran relay. Remove the entire
`[websocket]` section from application manifests, including listener addresses,
TLS certificate/key paths, private CA configuration and `allow_insecure`.
The parser rejects this retired section with migration guidance.

Provision TLS at the Loran relay endpoint and use secure signaling/bootstrap
URLs. Relay discovery from `wss://` signaling rejects a plaintext relay URL;
normal TLS certificate-chain and endpoint-name verification remains enabled.
The SDK authenticates peer identities and encrypts application payloads with
Noise, in addition to transport TLS. Do not restore direct peer listeners or
send reusable actor credentials in peer WebSocket upgrade headers.

Upgrade AIS/Loran before the mutually communicating SDKs. See the
[relay design](../wss-relay-design.zh.md),
[acceptance coverage](../wss-relay-acceptance.zh.md) and
[Loran configuration](../../../loran/config/README.md) for deployment details.
