import type { OperationBatch } from '../../shared/types';

/** Clone-safe error representation used across the native guest worker seam. */
export interface NativeWorkerError {
  name: string;
  message: string;
  stack?: string;
  /** Strict Host API v0.1 fields; omitted for ordinary implementation errors. */
  method?: string;
  code?: string;
  retryable?: boolean;
}

export type MainToNativeWorkerMessage =
  | { type: 'init'; wasmBytes: ArrayBuffer }
  | { type: 'init'; wasmModule: WebAssembly.Module }
  | {
      type: 'load';
      turnId: number;
      initialProps?: Record<string, unknown>;
    }
  // Reason: event payloads are contract-defined structured-clone values.
  | { type: 'event'; turnId: number; name: string; payload?: unknown }
  | {
      type: 'hostResult';
      callId: number;
      ok: boolean;
      // Reason: capability results are validated by the main-thread contract.
      result?: unknown;
      error?: NativeWorkerError;
    }
  | { type: 'memoryRead'; requestId: number; ptr: number; len: number }
  | { type: 'memoryWrite'; requestId: number; ptr: number; bytes: ArrayBuffer }
  | { type: 'destroy' };

export type NativeWorkerToMainMessage =
  | { type: 'ready' }
  | { type: 'loaded'; turnId: number; abiVersion: number | null }
  | { type: 'turnDone'; turnId: number }
  | { type: 'turnError'; turnId: number; error: NativeWorkerError }
  | { type: 'guestState'; turnId: number; running: boolean }
  | {
      type: 'hostCall';
      turnId: number;
      callId: number;
      moduleId: string;
      method: string;
      // Reason: capability input is decoded in the worker and validated on the main thread.
      input: unknown;
    }
  | { type: 'hostCancel'; callId: number }
  | {
      type: 'memoryReadResult';
      requestId: number;
      ok: boolean;
      bytes?: ArrayBuffer;
      error?: NativeWorkerError;
    }
  | {
      type: 'memoryWriteResult';
      requestId: number;
      ok: boolean;
      error?: NativeWorkerError;
    }
  | { type: 'batch'; batch: OperationBatch }
  | { type: 'log'; message: string };

// Reason: JavaScript permits throwing arbitrary values across every async boundary.
export function serializeNativeWorkerError(value: unknown): NativeWorkerError {
  const error = value instanceof Error ? value : new Error(String(value));
  const candidate = value as { method?: unknown; code?: unknown; retryable?: unknown } | null;
  const structured =
    candidate &&
    typeof candidate.method === 'string' &&
    typeof candidate.code === 'string' &&
    /^[a-z][a-z0-9-]{0,63}$/.test(candidate.code) &&
    typeof candidate.retryable === 'boolean'
      ? {
          method: candidate.method,
          code: candidate.code,
          retryable: candidate.retryable,
        }
      : {};
  return { name: error.name, message: error.message, stack: error.stack, ...structured };
}

export function deserializeNativeWorkerError(value: NativeWorkerError): Error {
  const error = new Error(value.message);
  error.name = value.name;
  if (value.stack) error.stack = value.stack;
  if (
    typeof value.method === 'string' &&
    typeof value.code === 'string' &&
    /^[a-z][a-z0-9-]{0,63}$/.test(value.code) &&
    typeof value.retryable === 'boolean'
  ) {
    Object.assign(error, {
      method: value.method,
      code: value.code,
      retryable: value.retryable,
    });
  }
  return error;
}
