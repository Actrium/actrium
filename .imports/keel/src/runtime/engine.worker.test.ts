import { describe, expect, it } from 'bun:test';
import { WorkerProvider } from '../sandbox/index';
import { Engine } from './engine';

// This is a smoke test for the evalAsync plumbing with WorkerProvider.
// It uses a fake Worker that echoes 'result' without actual code execution.

class FakeWorker {
  onmessage: ((ev: MessageEvent<any>) => void) | null = null;
  postMessage(msg: any) {
    // Handle init message
    if (msg.type === 'init') {
      queueMicrotask(() => {
        this.onmessage?.({ data: { type: 'result', id: msg.id, result: null } } as any);
      });
    }
    // Simulate immediate success for eval
    if (msg.type === 'eval') {
      queueMicrotask(() => {
        this.onmessage?.({ data: { type: 'result', id: msg.id, result: null } } as any);
      });
    }
  }
  terminate() {}
}

function createFakeWorker() {
  return new FakeWorker() as unknown as Worker;
}

describe('Engine with WorkerProvider (skeleton)', () => {
  it('should call evalAsync via worker provider without throwing', async () => {
    const provider = new WorkerProvider(createFakeWorker);
    const engine = new Engine({ quickjs: provider, debug: false });

    await engine.loadBundle('/* code */');
    expect(engine.isLoaded).toBe(true);
    engine.destroy();
  });

  it('should handle errors in worker provider', async () => {
    class ErrorWorker extends FakeWorker {
      postMessage(msg: any) {
        if (msg.type === 'init') {
          queueMicrotask(() => {
            this.onmessage?.({ data: { type: 'result', id: msg.id, result: null } } as any);
          });
        } else if (msg.type === 'eval') {
          queueMicrotask(() => {
            // Fail only on the specific user test code, not on shim code that also contains throw
            const isUserCode = msg.code?.trim() === 'throw new Error()';
            if (isUserCode) {
              this.onmessage?.({
                data: {
                  type: 'error',
                  id: msg.id,
                  error: { name: 'Error', message: 'Eval error', stack: '' },
                },
              } as any);
            } else {
              this.onmessage?.({ data: { type: 'result', id: msg.id, result: null } } as any);
            }
          });
        }
      }
    }

    const provider = new WorkerProvider(() => new ErrorWorker() as unknown as Worker);
    const engine = new Engine({ quickjs: provider, debug: false });

    await expect(engine.loadBundle('throw new Error()')).rejects.toThrow();
    engine.destroy();
  });

  it('should support setGlobal operations', async () => {
    class TrackingWorker extends FakeWorker {
      setGlobalCalls: any[] = [];
      postMessage(msg: any) {
        if (msg.type === 'init') {
          queueMicrotask(() => {
            this.onmessage?.({ data: { type: 'result', id: msg.id, result: null } } as any);
          });
        } else if (msg.type === 'setGlobal') {
          this.setGlobalCalls.push({ name: msg.name, value: msg.value });
        } else if (msg.type === 'eval') {
          queueMicrotask(() => {
            this.onmessage?.({ data: { type: 'result', id: msg.id, result: null } } as any);
          });
        }
      }
    }

    const trackingWorker = new TrackingWorker();
    const provider = new WorkerProvider(() => trackingWorker as unknown as Worker);
    const runtime = await provider.createRuntime();
    const context = runtime.createContext();

    // Call setGlobal with a serializable value
    context.setGlobal('testValue', 42);
    expect(trackingWorker.setGlobalCalls.length).toBe(1);
    expect(trackingWorker.setGlobalCalls[0]).toEqual({ name: 'testValue', value: 42 });

    context.dispose();
  });

  it('should handle timeout option', async () => {
    const provider = new WorkerProvider(createFakeWorker, { timeout: 1000 });
    const engine = new Engine({ quickjs: provider, debug: false });

    await engine.loadBundle('/* code */');
    expect(engine.isLoaded).toBe(true);
    engine.destroy();
  });
});
