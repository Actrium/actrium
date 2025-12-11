# Rill Roadmap TODOs

This document tracks improvement items that are identified but not yet completed. After each feature/update, we will re-run analysis and keep this list in sync.

## P0 – Reliability & Correctness
- [ ] WorkerQuickJSProvider (evalAsync skeleton)
  - [x] Introduce evalAsync path in Engine (fallback to sync eval)
  - [x] Add WorkerQuickJSProvider skeleton with proxy runtime/context
  - [x] Add basic smoke test with fake Worker
  - [ ] Implement real worker script wired to QuickJS (emscripten or RN)
  - [ ] Add e2e tests: worker isolates blocking guest; main thread remains responsive

- [ ] SDK runtime dependency guard – expand coverage
  - [ ] Add CI job artifacts for analyze.report.json to aid failure diagnostics
  - [ ] Enhance dependency report with source locations (line/column) where possible
- [x] Host↔Guest event channel completeness
  - [x] Inject `__useHostEvent` in Engine runtime with proper subscription lifecycle (includes unsubscribe)
  - [x] Add integration tests: host → guest events trigger registered guest handlers
- [ ] API/document alignment
  - [ ] Unify docs and examples to use `engine.register(...)`, `engine.loadBundle(...)`, `new Engine({ quickjs })`
  - [ ] Provide a minimal host integration sample with QuickJSProvider and component registration
- [ ] Execution safety (QuickJS)
  - [ ] Explore QuickJS interrupt handler/execution budget to allow hard timeouts
  - [ ] If unavailable, prototype running QuickJS in a separate thread/worker with kill/restart capability
  - [ ] Add policy: on timeout → destroy runtime and emit fatal error to host

## P1 – Backpressure, Security, Observability
- [x] Receiver backpressure & consistency: send RECEIVER_BACKPRESSURE event to guest instead of silent drop
  - [x] Add unit test for backpressure event
- [ ] Merge/compact batched updates in reconciler; expose counters and thresholds
- [ ] Receiver backpressure & consistency
  - [ ] Replace silent drops (if any) with explicit error/backpressure messages to guest
  - [ ] Merge/compact batched updates in reconciler; expose counters and thresholds
- [ ] Security hardening
  - [ ] Avoid injecting full `react-native` object into sandbox; provide restricted, frozen adapter layer
  - [ ] Review and minimize require whitelist; consider removing `react-native` entirely for guest
- [ ] Observability & diagnostics
  - [ ] Standardize `onMetric` names and levels; provide default console reporter
  - [ ] Add EngineView DevOverlay (dev-only) to show health, metrics, recent errors, event flow
  - [ ] Log version snapshot (React, RN, rill, reconciler) on Engine start

## P2 – Developer Experience
- [ ] Default component registration helper
  - [ ] Provide `registerDefaultComponents(engine)` or equivalent mapping to reduce boilerplate
- [ ] Docs & templates
  - [ ] Author a "SDK compile-time inlining" best-practice page with Vite/TS config samples
  - [ ] Update `init` template README with strict guard explanation and troubleshooting
  - [ ] Provide version compatibility matrix and keep CI aligned
- [ ] Auto-recovery & safe mode
  - [ ] Optional automatic restart strategy with circuit-breaker
  - [ ] Host UI: manual retry button and safe-mode (e.g., disable animations/large assets)

## CI & Tooling
- [ ] CI workflows
  - [x] Migrate to Bun for install, typecheck, build, test
  - [x] Add strict-guard build + analyze steps in CI
  - [ ] Split CLI e2e tests into a separate job; upload analyze reports on failure
- [ ] CLI analyze improvements (ongoing)
  - [x] Add `analyze` command
  - [x] Add strict guard integration tests
  - [ ] Provide more detailed JSON report (locations, module graph)
  - [ ] Upload analyze.report.json in CI on failures (artifacts)

## Canvas / Packaging
- [ ] Integrate strict guard into Canvas/TestData pack scripts (`pack-actr.sh`) (planned)
  - [ ] Run `rill analyze --fail-on-violation` on produced bundles
  - [ ] Fail fast with clear error messaging when `rill/sdk` is present at runtime

## Nice-to-haves
- [ ] Engine scenario samples (timeout, backpressure, error recovery) for regression testing
- [ ] Perf budget and stress test suites for reconciler/receiver
- [ ] Telemetry hooks (opt-in) for guest performance in production

## Default QuickJS Provider (WASM)
- [x] Design a default provider based on quickjs-emscripten, exported as `DefaultQuickJSProvider`
  - [ ] Implement using WorkerQuickJSProvider wrapper + worker.quickjs.emscripten.js
  - [ ] Expose `new Engine({ quickjs: DefaultQuickJSProvider })` as a ready-to-use option
  - [ ] Allow hosts to override with platform providers (e.g., react-native-quick-js)
  - [ ] Environment detection and fallback (Node/Web → WASM Worker; RN → require host provider)
  - [ ] Document trade-offs: bundle size, performance, interrupt support, security boundaries
- [ ] Tests/CI
  - [ ] e2e with DefaultQuickJSProvider (simple eval, dead-loop timeout, JSX shim)
  - [ ] CI job to install quickjs-emscripten and run worker e2e suite

## DevOverlay & Diagnostics (optional enhancements)
- [ ] Integrate DevOverlay into EngineView (dev-only feature flag)
- [ ] Add in-memory metrics buffer (last N records) and pass to DevOverlay
- [ ] Show backpressure counts and last errors in overlay
- [ ] CI failure artifacts: upload analyze.report.json and recent metrics dump on failures

---
Maintenance rule: After each merged PR touching Engine/CLI/SDK/Docs, re-run the checklist, update statuses, and add new findings.
