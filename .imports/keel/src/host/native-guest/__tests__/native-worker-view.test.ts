import { afterEach, describe, expect, it } from 'bun:test';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import {
  defineKeelContract,
  implementHostModules,
  rpc,
} from '../../../contract';
import {
  NativeGuestWorkerView,
  type NativeGuestWorkerOptions,
} from '../native-worker-view';
import type {
  MainToNativeWorkerMessage,
  NativeWorkerToMainMessage,
} from '../native-worker-protocol';

const UI_GUEST = readFileSync(join(import.meta.dir, 'fixtures/ui-guest.wasm'));
const ASSET_GUEST = readFileSync(join(import.meta.dir, 'fixtures/asset-guest.wasm'));
const RUNAWAY_GUEST = readFileSync(join(import.meta.dir, 'fixtures/runaway.wasm'));
const CANCEL_CALL = readFileSync(join(import.meta.dir, 'fixtures/cancel-call.wasm'));

const emptyContract = defineKeelContract({
  version: '1',
  hostModules: {},
  guestExports: {},
});

const makeWorker = (): Worker =>
  new Worker(new URL('../native-worker-host.ts', import.meta.url).href, {
    type: 'module',
  });

const components = {
  // biome-ignore lint/suspicious/noExplicitAny: opaque test materializers.
  View: 'View' as any,
  // biome-ignore lint/suspicious/noExplicitAny: opaque test materializers.
  Text: 'Text' as any,
};

function makeEngine(
  options: Omit<NativeGuestWorkerOptions, 'components' | 'createWorker'>
): NativeGuestWorkerView {
  return new NativeGuestWorkerView({ ...options, components, createWorker: makeWorker });
}

class RunawayEventWorker {
  onmessage: ((event: MessageEvent<NativeWorkerToMainMessage>) => void) | null = null;
  onerror: ((event: ErrorEvent) => void) | null = null;
  terminated = false;

  postMessage(message: MainToNativeWorkerMessage): void {
    if (message.type === 'init') {
      this.emit({ type: 'ready' });
    } else if (message.type === 'load') {
      this.emit({ type: 'loaded', turnId: message.turnId, abiVersion: null });
    } else if (message.type === 'event') {
      this.emit({ type: 'guestState', turnId: message.turnId, running: true });
    }
  }

  terminate(): void {
    this.terminated = true;
  }

  private emit(message: NativeWorkerToMainMessage): void {
    queueMicrotask(() =>
      this.onmessage?.({ data: message } as MessageEvent<NativeWorkerToMainMessage>)
    );
  }
}

describe('NativeGuestWorkerView — off-main-thread native guest', () => {
  let engine: NativeGuestWorkerView | null = null;

  afterEach(() => {
    engine?.destroy();
    engine = null;
  });

  it('loads and renders a real native guest through the worker', async () => {
    engine = makeEngine({
      wasmBytes: UI_GUEST,
      contract: emptyContract,
      hostModules: {},
      watchdogTimeout: 20_000,
    });
    const receiver = engine.createReceiver();

    await engine.loadBundle();

    expect(engine.isLoaded).toBe(true);
    expect(engine.guestAbiVersion).toBe(1);
    expect(engine.exports).toBeUndefined();
    const tree = receiver.getComponentTree();
    expect(tree?.type).toBe('View');
    expect(tree?.children[0].type).toBe('__TEXT__');
    expect(tree?.children[0].props.text).toBe('hello from rust');
    let settled = false;

    // ui-guest has no named-event export, but the worker still owes an explicit
    // turn completion. The promise must not settle at postMessage enqueue time.
    const handled = engine.sendEventAndWait('noop', { value: 7 });
    void handled.then(() => { settled = true; });
    await Promise.resolve();

    expect(settled).toBe(false);
    await handled;
    expect(settled).toBe(true);
  });

  it('accepts an already-compiled module without turning it back into bytes', async () => {
    engine = makeEngine({
      wasmModule: await WebAssembly.compile(UI_GUEST),
      contract: emptyContract,
      hostModules: {},
      watchdogTimeout: 20_000,
    });
    const receiver = engine.createReceiver();

    await engine.loadBundle();

    expect(engine.isLoaded).toBe(true);
    expect(receiver.getComponentTree()?.children[0].props.text).toBe('hello from rust');
  });

  it('round-trips bounded pointer reads and writes without mirroring the whole heap', async () => {
    type Input = { assetId: string; dstPtr: number; dstCap: number };
    type Output = { ok: boolean; written?: number; width?: number; height?: number };
    const contract = defineKeelContract({
      version: '1',
      hostModules: {
        'host:asset': {
          info: rpc<Record<string, unknown>, Record<string, unknown>>(),
          blit: rpc<Input, Output>(),
        },
      },
      guestExports: {},
    });
    let sawInitialZeros = false;
    let sawWrittenBytes = false;
    const pattern = Uint8Array.from({ length: 16 }, (_, index) => index * 7 + 1);
    const hostModules = implementHostModules(contract, {
      'host:asset': {
        info: async () => ({ width: 2, height: 2 }),
        blit: async (input) => {
          if (!engine) throw new Error('engine unavailable');
          const before = await engine.readGuestMemory(input.dstPtr, pattern.length);
          sawInitialZeros = before.every((value) => value === 0);
          await engine.writeGuestMemory(input.dstPtr, pattern);
          const after = await engine.readGuestMemory(input.dstPtr, pattern.length);
          sawWrittenBytes = after.every((value, index) => value === pattern[index]);
          return { ok: true, written: pattern.length };
        },
      },
    });
    engine = makeEngine({
      wasmBytes: ASSET_GUEST,
      contract,
      hostModules,
      watchdogTimeout: 20_000,
    });
    engine.createReceiver();

    await engine.loadBundle();

    expect(sawInitialZeros).toBe(true);
    expect(sawWrittenBytes).toBe(true);
  });

  it('does not charge time spent awaiting a host capability to the guest CPU budget', async () => {
    const contract = defineKeelContract({
      version: '1',
      hostModules: {
        'host:asset': {
          info: rpc<Record<string, unknown>, Record<string, unknown>>(),
          blit: rpc<Record<string, unknown>, Record<string, unknown>>(),
        },
      },
      guestExports: {},
    });
    const hostModules = implementHostModules(contract, {
      'host:asset': {
        info: async () => {
          await new Promise((resolve) => setTimeout(resolve, 800));
          return { width: 2, height: 2 };
        },
        blit: async () => ({ ok: false }),
      },
    });
    let killed = false;
    engine = makeEngine({
      wasmBytes: ASSET_GUEST,
      contract,
      hostModules,
      watchdogTimeout: 500,
      onWatchdogKill: () => {
        killed = true;
      },
    });
    engine.createReceiver();

    await engine.loadBundle();

    expect(killed).toBe(false);
    expect(engine.isLoaded).toBe(true);
  });

  it('routes native cancellation through the worker to the matching host call', async () => {
    const contract = defineKeelContract({
      version: '1',
      hostModules: {
        'host:test': {
          wait: rpc<{ value: number }, { ok: true }>({
            schema: {
              parseInput: (value) => ({ value: Number((value as { value?: unknown }).value) }),
            },
          }),
        },
      },
      guestExports: {},
    });
    let sawAbort = false;
    const hostModules = implementHostModules(contract, {
      'host:test': {
        wait: (_input: unknown, context: { signal: AbortSignal }) =>
          new Promise((_resolve, reject) => {
            const abort = () => {
              sawAbort = true;
              reject(new Error('cancelled'));
            };
            if (context.signal.aborted) abort();
            else context.signal.addEventListener('abort', abort, { once: true });
          }),
      },
    });
    engine = makeEngine({
      wasmBytes: CANCEL_CALL,
      contract,
      hostModules,
      watchdogTimeout: 20_000,
    });

    await engine.loadBundle();

    expect(sawAbort).toBe(true);
  });

  it('hard-kills a synchronous native guest loop while the main thread stays live', async () => {
    let killed = false;
    engine = makeEngine({
      wasmBytes: RUNAWAY_GUEST,
      contract: emptyContract,
      hostModules: {},
      watchdogTimeout: 250,
      onWatchdogKill: () => {
        killed = true;
      },
    });
    engine.createReceiver();

    await expect(engine.loadBundle()).rejects.toThrow(/watchdog/);

    expect(killed).toBe(true);
    expect(engine.isDestroyed).toBe(true);
  });

  it('charges an event turn to its own watchdog and hard-kills its worker', async () => {
    let killedTurn = 0;
    const worker = new RunawayEventWorker();
    engine = new NativeGuestWorkerView({
      wasmBytes: new Uint8Array(),
      contract: emptyContract,
      hostModules: {},
      components,
      createWorker: () => worker,
      watchdogTimeout: 20,
      onWatchdogKill: ({ turnId }) => {
        killedTurn = turnId;
      },
    });
    engine.createReceiver();
    await engine.loadBundle();

    await expect(engine.sendEventAndWait('runaway')).rejects.toThrow(/watchdog/);

    expect(killedTurn).toBeGreaterThan(1);
    expect(engine.isDestroyed).toBe(true);
    expect(worker.terminated).toBe(true);
  });

  it('terminates the worker even when the watchdog observer throws', async () => {
    let observerCalled = false;
    engine = makeEngine({
      wasmBytes: RUNAWAY_GUEST,
      contract: emptyContract,
      hostModules: {},
      watchdogTimeout: 100,
      onWatchdogKill: () => {
        observerCalled = true;
        throw new Error('observer failed');
      },
    });
    engine.createReceiver();

    await expect(engine.loadBundle()).rejects.toThrow(/watchdog/);

    expect(observerCalled).toBe(true);
    expect(engine.isDestroyed).toBe(true);
  });
});
