import type { HostModuleDispatchTable } from '../../contract';
import { NativeGuestHost } from './native-guest-host';
import type {
  MainToNativeWorkerMessage,
  NativeWorkerToMainMessage,
} from './native-worker-protocol';
import { serializeNativeWorkerError } from './native-worker-protocol';

export interface NativeWorkerDispatchOptions {
  post(message: NativeWorkerToMainMessage, transfer?: Transferable[]): void;
}

/**
 * Worker-side native guest driver. Control replies and memory RPCs bypass the
 * turn queue; guest entry points themselves are serialized.
 */
export class NativeWorkerDispatch {
  private readonly post: NativeWorkerDispatchOptions['post'];
  private wasmSource: ArrayBuffer | WebAssembly.Module | null = null;
  private host: NativeGuestHost | null = null;
  private currentTurnId = 0;
  private callSeq = 0;
  private destroyed = false;
  private turnQueue: Promise<void> = Promise.resolve();
  private readonly pendingHostCalls = new Map<
    number,
    // Reason: capability inputs and results are contract-defined values decoded at the boundary.
    { resolve(value: unknown): void; reject(error: Error): void }
  >();

  constructor(options: NativeWorkerDispatchOptions) {
    this.post = options.post;
  }

  handle(message: MainToNativeWorkerMessage): void {
    if (this.destroyed && message.type !== 'destroy') return;
    switch (message.type) {
      case 'init':
        this.wasmSource = 'wasmModule' in message ? message.wasmModule : message.wasmBytes;
        this.post({ type: 'ready' });
        return;
      case 'hostResult':
        this.resolveHostCall(message);
        return;
      case 'memoryRead':
        this.readMemory(message.requestId, message.ptr, message.len);
        return;
      case 'memoryWrite':
        this.writeMemory(message.requestId, message.ptr, message.bytes);
        return;
      case 'destroy':
        this.destroy();
        return;
      case 'load':
      case 'event':
        this.turnQueue = this.turnQueue.then(() => this.runTurn(message));
        return;
    }
  }

  private async runTurn(
    message: Extract<MainToNativeWorkerMessage, { type: 'load' | 'event' }>
  ): Promise<void> {
    if (this.destroyed) return;
    this.currentTurnId = message.turnId;
    try {
      if (message.type === 'load') {
        if (!this.wasmSource) throw new Error('native worker was not initialized');
        if (this.host) throw new Error('native guest is already loaded');
        this.host = new NativeGuestHost({
          dispatch: this.createProxyDispatch(),
          onLog: (value) => this.post({ type: 'log', message: value }),
          onRenderBatch: (batch) => this.post({ type: 'batch', batch }),
          onGuestRunStateChange: (running) =>
            this.post({ type: 'guestState', turnId: message.turnId, running }),
        });
        await this.host.load(this.wasmSource);
        if (message.initialProps !== undefined) {
          this.host.emitEvent('props', message.initialProps);
        }
        await this.host.drain();
        this.post({
          type: 'loaded',
          turnId: message.turnId,
          abiVersion: this.host.guestAbiVersion,
        });
      } else {
        if (!this.host) throw new Error('native guest is not loaded');
        this.host.emitEvent(message.name, message.payload);
        await this.host.drain();
        this.post({ type: 'turnDone', turnId: message.turnId });
      }
    } catch (error) {
      this.post({
        type: 'turnError',
        turnId: message.turnId,
        error: serializeNativeWorkerError(error),
      });
    } finally {
      this.currentTurnId = 0;
    }
  }

  private createProxyDispatch(): HostModuleDispatchTable {
    return new Proxy(
      {},
      {
        get: (_target, moduleId) => {
          if (typeof moduleId !== 'string') return undefined;
          return new Proxy(
            {},
            {
              get: (_module, method) => {
                if (typeof method !== 'string') return undefined;
                // Reason: the main-thread contract validates this capability input.
                return (input: unknown) => this.callHost(moduleId, method, input);
              },
            }
          );
        },
      }
    ) as HostModuleDispatchTable;
  }

  // Reason: capability inputs and results are opaque until the main-thread contract validates them.
  private callHost(moduleId: string, method: string, input: unknown): Promise<unknown> {
    if (!this.currentTurnId) return Promise.reject(new Error('host call outside a guest turn'));
    const callId = ++this.callSeq;
    return new Promise((resolve, reject) => {
      this.pendingHostCalls.set(callId, { resolve, reject });
      this.post({
        type: 'hostCall',
        turnId: this.currentTurnId,
        callId,
        moduleId,
        method,
        input,
      });
    });
  }

  private resolveHostCall(
    message: Extract<MainToNativeWorkerMessage, { type: 'hostResult' }>
  ): void {
    const pending = this.pendingHostCalls.get(message.callId);
    if (!pending) return;
    this.pendingHostCalls.delete(message.callId);
    if (message.ok) pending.resolve(message.result);
    else
      pending.reject(
        message.error ? new Error(message.error.message) : new Error('host call failed')
      );
  }

  private readMemory(requestId: number, ptr: number, len: number): void {
    try {
      if (!this.host) throw new Error('native guest is not loaded');
      const bytes = this.host.readBytes(ptr, len);
      const buffer = bytes.buffer as ArrayBuffer;
      this.post({ type: 'memoryReadResult', requestId, ok: true, bytes: buffer }, [buffer]);
    } catch (error) {
      this.post({
        type: 'memoryReadResult',
        requestId,
        ok: false,
        error: serializeNativeWorkerError(error),
      });
    }
  }

  private writeMemory(requestId: number, ptr: number, buffer: ArrayBuffer): void {
    try {
      if (!this.host) throw new Error('native guest is not loaded');
      this.host.writeBytes(ptr, new Uint8Array(buffer));
      this.post({ type: 'memoryWriteResult', requestId, ok: true });
    } catch (error) {
      this.post({
        type: 'memoryWriteResult',
        requestId,
        ok: false,
        error: serializeNativeWorkerError(error),
      });
    }
  }

  private destroy(): void {
    this.destroyed = true;
    for (const pending of this.pendingHostCalls.values()) {
      pending.reject(new Error('native worker destroyed'));
    }
    this.pendingHostCalls.clear();
    this.host = null;
  }
}
