// engine.worker.ts - Worker script for running QuickJS via @sebastianwessel/quickjs v3.0.0

import { loadQuickJs } from "@sebastianwessel/quickjs";
import variant from "@jitl/quickjs-wasmfile-release-sync";

interface WorkerOptions {
  timeout?: number;
}

let sandboxReady: Promise<{
  runSandboxed: Awaited<ReturnType<typeof loadQuickJs>>["runSandboxed"];
}> | null = null;
let workerOptions: WorkerOptions = {};

async function getSandbox() {
  if (!sandboxReady) {
    sandboxReady = loadQuickJs(variant).then(({ runSandboxed }) => ({
      runSandboxed,
    }));
  }
  return sandboxReady;
}

// Store globals that need to be set before eval
const pendingGlobals = new Map<string, unknown>();

// Built-in shims that will be injected into every eval
// These provide React/JSX runtime compatibility in the QuickJS sandbox
const BUILTIN_SHIMS = `
// React shim
globalThis.React = {
  createElement: function(type, props) {
    var children = Array.prototype.slice.call(arguments, 2);
    return { $$typeof: Symbol.for('react.element'), type: type, props: Object.assign({}, props, { children: children.length === 1 ? children[0] : children }), key: props && props.key || null };
  },
  Fragment: Symbol.for('react.fragment'),
  useState: function(init) { return [init, function() {}]; },
  useEffect: function() {},
  useCallback: function(fn) { return fn; },
  useMemo: function(fn) { return fn(); },
  useRef: function(init) { return { current: init }; },
  useContext: function() { return undefined; },
  createContext: function(def) { return { Provider: 'Provider', Consumer: 'Consumer', _currentValue: def }; },
  memo: function(c) { return c; },
  forwardRef: function(c) { return c; },
};

// ReactJSXRuntime shim
globalThis.ReactJSXRuntime = {
  jsx: function(type, props, key) {
    var p = Object.assign({}, props);
    if (key !== undefined) p.key = key;
    return { $$typeof: Symbol.for('react.element'), type: type, props: p, key: key || null };
  },
  jsxs: function(type, props, key) {
    var p = Object.assign({}, props);
    if (key !== undefined) p.key = key;
    return { $$typeof: Symbol.for('react.element'), type: type, props: p, key: key || null };
  },
  Fragment: Symbol.for('react.fragment'),
};

// Console shim (output is lost in sandbox but prevents errors)
globalThis.console = {
  log: function() {},
  warn: function() {},
  error: function() {},
  debug: function() {},
  info: function() {},
};

// require shim
globalThis.require = function(moduleName) {
  if (moduleName === 'react') return globalThis.React;
  if (moduleName === 'react/jsx-runtime') return globalThis.ReactJSXRuntime;
  throw new Error('[worker] Unsupported require: ' + moduleName);
};
`;

self.onmessage = async (ev: MessageEvent) => {
  const { type, id, code, name, value, options } = ev.data;

  try {
    switch (type) {
      case "init":
        // Store options for later use
        if (options?.timeout) {
          workerOptions.timeout = options.timeout;
        }
        // Pre-load the sandbox
        await getSandbox();
        (self as any).postMessage({ type: "result", id, result: "ready" });
        break;

      case "eval":
        if (code) {
          const { runSandboxed } = await getSandbox();

          // Build code: shims + pending globals + user code
          let fullCode = BUILTIN_SHIMS;
          for (const [gName, gValue] of pendingGlobals) {
            fullCode += `globalThis.${gName} = ${JSON.stringify(gValue)};\n`;
          }
          fullCode += code;

          const result = await runSandboxed(
            async ({ evalCode }) => {
              return await evalCode(fullCode);
            },
            {
              executionTimeout: workerOptions.timeout,
            }
          );

          if (result.ok) {
            (self as any).postMessage({ type: "result", id, result: result.data });
          } else {
            (self as any).postMessage({
              type: "error",
              id,
              error: result.error?.message || "Evaluation failed",
            });
          }
        }
        break;

      case "dispose":
        // In v3 API, sandbox is disposed after each runSandboxed call
        // Just reset state
        sandboxReady = null;
        pendingGlobals.clear();
        (self as any).postMessage({ type: "result", id, result: "disposed" });
        break;

      case "setGlobal":
        // Store for injection during eval
        pendingGlobals.set(name, value);
        // No response needed for fire-and-forget
        break;

      case "getGlobal":
        // In v3 API we can't easily get globals between sandbox runs
        // since each runSandboxed creates a fresh context.
        // Return undefined or stored value
        const storedValue = pendingGlobals.get(name);
        (self as any).postMessage({ type: "result", id, result: storedValue });
        break;
    }
  } catch (error) {
    const err = error instanceof Error ? error.message : String(error);
    (self as any).postMessage({ type: "error", id, error: err });
  }
};
