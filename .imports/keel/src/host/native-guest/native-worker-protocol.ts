import type { OperationBatch } from '../../shared/types';

/** Clone-safe error representation used across the native guest worker seam. */
export interface NativeWorkerError {
  name: string;
  message: string;
  stack?: string;
}

export type MainToNativeWorkerMessage =
  | { type: 'init'; wasmBytes: ArrayBuffer }
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
  return { name: error.name, message: error.message, stack: error.stack };
}

export function deserializeNativeWorkerError(value: NativeWorkerError): Error {
  const error = new Error(value.message);
  error.name = value.name;
  if (value.stack) error.stack = value.stack;
  return error;
}
