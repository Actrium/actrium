import { describe, it, expect } from 'bun:test';

import { Engine } from './engine';
import { WorkerJSEngineProvider } from './WorkerJSEngineProvider';

function createWorker(){
  const url = new URL('./engine.worker.ts', import.meta.url);
  // @ts-ignore
  return new Worker(url, { type: 'module' });
}

describe('WorkerJSEngineProvider React/JSXRuntime rich shim e2e', () => {
  it.skipIf(typeof Worker === 'undefined')('should evaluate basic React.createElement and ReactJSXRuntime output', async () => {
    const provider = new WorkerJSEngineProvider(createWorker, { timeout: 200 });
    const engine = new Engine({ quickjs: provider, debug: false });

    const code = `
      const a = React.createElement('Text', { x: 1 }, 'hi');
      const b = ReactJSXRuntime.jsx('View', { id: 'ok', children: 'hello' });
    `;

    await engine.loadBundle(code);
    expect(engine.isLoaded).toBe(true);
    engine.destroy();
  });
});
