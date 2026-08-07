# Error Handling

Actor-RTC Web treats errors as structured runtime signals rather than ad hoc console noise. This guidance applies to the current Option U / wasm-bindgen browser path.

## Main Error Categories

### Configuration Errors

Examples:

- invalid signaling or AIS URL
- missing realm, package, or actor identifiers
- incorrect Service Worker path
- missing `.actr` package, package binary, or signed `resources/glue.js`
- empty trust configuration, registry-only trust, or a `pubkey_file` not resolved to browser-usable `pubkey_b64`

These should fail fast during initialization whenever possible. Trust failures are fail-closed: the browser must not execute package content when no usable static anchor is available.

### Guest Loading Errors

Examples:

- `actor.sw.js` cannot extract or evaluate packaged `resources/glue.js`
- the package binary is missing or is not a usable wasm-bindgen `*_bg.wasm`
- `register_guest_workload` is not exported or throws
- the package signature or binary/resource hashes do not match

These are build or packaging errors. They should include the package URL, affected package entry, and original verification, JavaScript, or wasm-bindgen error.

### Connectivity Errors

Examples:

- WebSocket signaling failures
- AIS registration or discovery failures
- ICE negotiation failures
- WebRTC channel closure or setup timeouts

These should include enough transport context to show whether the failure happened during discovery, signaling, candidate exchange, or data-path setup.

### Browser Platform Errors

Examples:

- Service Worker registration failure
- MessagePort failure between DOM and Service Worker
- IndexedDB open, quota, or transaction failure
- browser security restrictions around origin, HTTPS, or storage mode

These should point to the browser feature or policy that blocked the runtime.

### Protocol Errors

Examples:

- malformed payloads
- route-key mismatches
- request/response correlation failures
- decode and encode failures

These should be treated as correctness bugs or wire-compatibility problems, not transient transport issues.

## Reporting Guidance

- Use structured Rust error types where possible.
- Use clear English log messages for browser-visible errors.
- Include actor, route, request, peer, client, or package identifiers when available.
- Prefer one high-signal error report over cascades of duplicate warnings.
- Keep DOM-side logs and Service Worker logs correlated through request or client identifiers.

## Recovery Guidance

- Retry only when the owning layer has a clear policy for doing so.
- Reconnect flows should be explicit and observable.
- Package layout, signature, or resource-hash errors should not be retried silently.
- Persistent storage corruption or schema mismatch should trigger an intentional reset path rather than undefined behavior.

## Documentation Expectations

Every externally visible failure mode should document:

- what failed
- where it failed
- whether it is retryable
- what the operator or developer should do next
