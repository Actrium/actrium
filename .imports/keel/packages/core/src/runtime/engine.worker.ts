// engine.worker.ts - Worker script for running QuickJS via @sebastianwessel/quickjs v3.0.0

import variant from '@jitl/quickjs-wasmfile-release-sync';
import { loadQuickJs } from '@sebastianwessel/quickjs';
import { ALL_SHIMS } from './shims';

interface WorkerOptions {
  timeout?: number;
}

let sandboxReady: Promise<{
  runSandboxed: Awaited<ReturnType<typeof loadQuickJs>>['runSandboxed'];
}> | null = null;
const workerOptions: WorkerOptions = {};

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

// Built-in shims - use shared implementation from shims.ts
// Add require shim for worker context
const BUILTIN_SHIMS = `
${ALL_SHIMS}

// require shim for worker context
globalThis.require = function(moduleName) {
  if (moduleName === 'react') return globalThis.React;
  if (moduleName === 'react/jsx-runtime') return globalThis.ReactJSXRuntime;
  throw new Error('[worker] Unsupported require: ' + moduleName);
};
`;

interface WorkerRequest {
  type: 'init' | 'eval' | 'dispose' | 'setGlobal';
  id?: string;
  code?: string;
  name?: string;
  value?: unknown;
  options?: { timeout?: number };
}

interface WorkerResponse {
  type: 'result' | 'error';
  id?: string;
  result?: unknown;
  error?: string;
}

self.onmessage = async (ev: MessageEvent<WorkerRequest>) => {
  const { type, id, code, name, value, options } = ev.data;

  const postResponse = (response: WorkerResponse) => {
    self.postMessage(response);
  };

  try {
    switch (type) {
      case 'init': {
        // Store options for later use
        if (options?.timeout) {
          workerOptions.timeout = options.timeout;
        }
        // Pre-load the sandbox
        await getSandbox();
        postResponse({ type: 'result', id, result: 'ready' });
        break;
      }

      case 'eval': {
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
            postResponse({ type: 'result', id, result: result.data });
          } else {
            postResponse({
              type: 'error',
              id,
              error: result.error?.message || 'Evaluation failed',
            });
          }
        }
        break;
      }

      case 'dispose': {
        // In v3 API, sandbox is disposed after each runSandboxed call
        // Just reset state
        sandboxReady = null;
        pendingGlobals.clear();
        postResponse({ type: 'result', id, result: 'disposed' });
        break;
      }

      case 'setGlobal': {
        // Store for injection during eval
        if (name !== undefined) {
          pendingGlobals.set(name, value);
        }
        // No response needed for fire-and-forget
        break;
      }
    }
  } catch (error) {
    const err = error instanceof Error ? error.message : String(error);
    postResponse({ type: 'error', id, error: err });
  }
};
