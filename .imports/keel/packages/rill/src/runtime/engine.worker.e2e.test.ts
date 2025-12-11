import { describe, it, expect } from 'bun:test';

import { Engine } from './engine';
import { WorkerJSEngineProvider } from './WorkerJSEngineProvider';

function createWorker(){
  // Bun supports new Worker(new URL('path', import.meta.url)) style
  const url = new URL('./engine.worker.ts', import.meta.url);
  // @ts-ignore - Bun Worker type compatible
  return new Worker(url, { type: 'module' });
}

describe('WorkerJSEngineProvider (e2e)', () => {
  it.skipIf(typeof Worker === 'undefined')('evaluates simple code in worker-backed QuickJS', async () => {
    const provider = new WorkerJSEngineProvider(createWorker);
    const engine = new Engine({ quickjs: provider, debug: false });

    await engine.loadBundle('var x = 1 + 2;');
    expect(engine.isLoaded).toBe(true);

    engine.destroy();
  });
});
