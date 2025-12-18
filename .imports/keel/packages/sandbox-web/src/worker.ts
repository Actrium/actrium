// rill/packages/sandbox-web/src/worker.ts

/**
 * This script runs inside the Web Worker.
 * It's a simple execution environment that receives code, runs it, and posts the result back.
 */

self.onmessage = async (event: MessageEvent<{ id: string; script: string }>) => {
  const { id, script } = event.data;

  try {
    // Using new Function() is safer than eval() as it doesn't have access to the surrounding scope.
    // In a worker, the global scope is already isolated from the main thread.
    const fn = new Function(`return (async () => { ${script} })()`);
    const result = await fn();
    // Post the result back to the main thread.
    self.postMessage({ id, result });
  } catch (error) {
    // If an error occurs, serialize it and post it back.
    const errorData =
      error instanceof Error
        ? { name: error.name, message: error.message, stack: error.stack }
        : { message: 'An unknown error occurred in the worker.' };
    self.postMessage({ id, error: errorData });
  }
};
