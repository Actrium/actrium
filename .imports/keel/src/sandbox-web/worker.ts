/**
 * Web Worker script for sandbox-web
 *
 * Executes JavaScript code in an isolated Web Worker environment.
 * Uses new Function() for code execution - the Worker itself provides isolation.
 */

interface WorkerMessage {
  type: 'eval' | 'setGlobal' | 'getGlobal' | 'dispose';
  id: string;
  code?: string;
  name?: string;
  value?: unknown;
}

interface WorkerResponse {
  id: string;
  result?: unknown;
  error?: { name: string; message: string; stack?: string };
}

// Global scope for sandboxed code
const sandbox: Record<string, unknown> = {};

const postResponse = (response: WorkerResponse) => {
  self.postMessage(response);
};

self.onmessage = async (event: MessageEvent<WorkerMessage>) => {
  const { type, id, code, name, value } = event.data;

  try {
    switch (type) {
      case 'eval': {
        if (code) {
          // Using new Function() is safer than eval() as it doesn't have access to the surrounding scope.
          // In a worker, the global scope is already isolated from the main thread.
          // Inject sandbox variables into the function scope
          const sandboxKeys = Object.keys(sandbox);
          const sandboxValues = Object.values(sandbox);

          const fn = new Function(...sandboxKeys, `return (async () => { ${code} })()`);
          const result = await fn(...sandboxValues);
          postResponse({ id, result });
        } else {
          postResponse({ id, result: undefined });
        }
        break;
      }

      case 'setGlobal': {
        if (name !== undefined) {
          sandbox[name] = value;
        }
        postResponse({ id, result: true });
        break;
      }

      case 'getGlobal': {
        const result = name !== undefined ? sandbox[name] : undefined;
        postResponse({ id, result });
        break;
      }

      case 'dispose': {
        // Clear sandbox
        for (const key of Object.keys(sandbox)) {
          delete sandbox[key];
        }
        postResponse({ id, result: true });
        break;
      }
    }
  } catch (error) {
    const errorData: { name: string; message: string; stack?: string } =
      error instanceof Error
        ? {
            name: error.name,
            message: error.message,
            ...(error.stack ? { stack: error.stack } : {}),
          }
        : { name: 'Error', message: String(error) };
    postResponse({ id, error: errorData });
  }
};
