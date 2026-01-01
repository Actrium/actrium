# Rill Production Readiness Guide

This guide summarizes recommended settings and operational practices to run Rill in production.

## 1. Runtime hardening

- Require whitelist
  - Pass `requireWhitelist` in EngineOptions when creating Engine. Only these modules can be `require()`ed by the guest bundle.
  - Default whitelist: `react`, `react-native`, `react/jsx-runtime`, `rill/let`.
- Execution timeout
  - Use `timeout` option (default 5000ms). QuickJS `eval` is synchronous and cannot be forcibly interrupted; this timeout is "best-effort". Consider worker/thread isolation if strict CPU time slicing is required.
- Error classification
  - The engine throws specific error classes: `RequireError`, `ExecutionError`, `TimeoutError`.
  - Subscribe to `engine.on('error', handler)` to log and report.
- Metrics hooks
  - Provide `onMetric(name, value, extra?)` to collect basic metrics:
    - `engine.resolveSource` (ms)
    - `engine.fetchBundle` (ms, { status, size })
    - `engine.initializeRuntime` (ms)
    - `engine.executeBundle` (ms, { size })
    - `receiver.applyBatch` (ms, { applied, skipped, total })
- Batch limits
  - `receiverMaxBatchSize` (default 5000) limits the number of operations applied in a single batch, preventing guests from overwhelming the host with massive operations. Excess operations are skipped and reported via metrics.

## 2. Observability

- Logging
  - Provide a structured logger via `logger` option. Enable `debug` in pre-production.
  - Surface guest-side logs via the injected `console`.
- Metrics
  - Forward `onMetric` events to your metrics system (Datadog/Prometheus/Logcat, etc.).
- Health checks
  - Implement a simple health probe that checks `engine.isLoaded` and, optionally, triggers a cheap `sendEvent` roundtrip.

## 3. Security & isolation

- Sandbox
  - Use a `JSEngineProvider` implementation (like QuickJS WASM) in production to ensure isolation from the host.
  - Supported sandbox modes: `'vm'` (Node.js VM), `'worker'` (Web Worker), `'none'` (no sandbox)
- Module access
  - Keep whitelist minimal; do not expose Node built-ins or dynamic loaders to the guest.
- Callbacks
  - Always validate payloads from the guest before applying to UI. Prefer strongly typed props.

## 4. Performance recommendations

- Throttling
  - Use `ThrottledScheduler` and `OperationMerger` when frequent updates are expected.
- Virtual lists
  - For long lists, wire `VirtualScrollCalculator` and `ScrollThrottler` with tuned parameters.
- Memory hygiene
  - Call `engine.destroy()` when the guest is no longer needed.

## 5. CI/CD & packaging

- Tests
  - Keep unit + integration tests green. Enforce coverage thresholds.
- Linting
  - `@typescript-eslint/consistent-type-imports` is enabled to avoid bundler parse edge cases.
- Bundles
  - Use CLI `build` with `--format=iife` by default for guests. Run `build analyze` to examine output.

## 6. Host integration checklist

- [ ] (Optional) Provide `JSEngineProvider` implementation, or use built-in sandbox mode
- [ ] Create `Engine` and configure `sandbox`, `requireWhitelist`, `timeout`, `logger`, `onMetric`, `receiverMaxBatchSize`
- [ ] Register host components: `engine.register()`
- [ ] Create and use `receiver = engine.createReceiver(onUpdate)` to render tree updates
- [ ] Wire event bridge: `engine.sendEvent` and `__handleHostMessage`
- [ ] Add health monitoring and metrics forwarding
- [ ] Subscribe to `engine.on('error', ...)` and provide fallback UI

## 7. Observability Details

- Metrics names and payloads
  - engine.resolveSource: { } in ms
  - engine.fetchBundle: { status, size } in ms
  - engine.initializeRuntime: { } in ms
  - engine.executeBundle: { size } in ms
  - engine.sendToSandbox: { size } in ms
  - receiver.applyBatch: { applied, skipped, failed, total } in ms
  - receiver.render: { nodeCount } in ms

- Using onMetric
```ts
const metrics: Array<{ name: string; value: number; extra?: Record<string, unknown> }> = [];
const engine = new Engine({
  provider: myJSEngineProvider, // Optional
  sandbox: 'vm', // Optional: 'vm' | 'worker' | 'none'
  onMetric: (n, v, e) => metrics.push({ name: n, value: v, extra: e })
});
```

- Health check API
```ts
const health = engine.getHealth();
// { loaded, destroyed, errorCount, lastErrorAt, receiverNodes }
```

## 8. CLI Usage

- Analyze with whitelist scanning
```bash
bun run rill/cli analyze dist/bundle.js # default warnings only
```
Programmatic (options):
```ts
import { analyze } from 'rill/cli';
await analyze('dist/bundle.js', {
  whitelist: ['react', 'react-native', 'react/jsx-runtime', 'rill/let'],
  failOnViolation: true,
  treatEvalAsViolation: true,
  treatDynamicNonLiteralAsViolation: true,
});
```

- Build guest bundle
```bash
bun run rill/cli build src/guest.tsx -o dist/bundle.js
```

## 9. Troubleshooting

- React Native in Node tests
  - Vitest maps `react-native` to a lightweight stub via an alias. This does not affect production bundles.
- Syntax errors like "Expected 'from', got 'typeOf'"
  - Ensure type-only imports are separated and lint rule is enabled.

