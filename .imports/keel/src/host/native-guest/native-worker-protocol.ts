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

const HOST_ERROR_CODE = /^[a-z][a-z0-9-]{0,62}$/;
const PUBLIC_HOST_ERROR_MESSAGE = 'Host API call failed';

interface StructuredHostErrorFields {
  method: string;
  code: string;
  retryable: boolean;
}

const structuredHostErrorFields = (
  // Reason: worker and host code may throw any JavaScript value across this message boundary.
  value: unknown,
  expectedMethod?: string
): StructuredHostErrorFields | null => {
  if (typeof value !== 'object' || value === null) return null;
  try {
    // Reason: every property of an arbitrary thrown value is untrusted until validated below.
    const candidate = value as { method?: unknown; code?: unknown; retryable?: unknown };
    const method = candidate.method;
    const code = candidate.code;
    const retryable = candidate.retryable;
    if (
      typeof method !== 'string' ||
      (expectedMethod !== undefined && method !== expectedMethod) ||
      typeof code !== 'string' ||
      !HOST_ERROR_CODE.test(code) ||
      typeof retryable !== 'boolean'
    ) {
      return null;
    }
    return { method, code, retryable };
  } catch {
    // An arbitrary thrown object may expose throwing accessors.
    return null;
  }
};

// Reason: ordinary worker failures may also be arbitrary JavaScript thrown values.
const ordinaryWorkerError = (value: unknown): NativeWorkerError => {
  try {
    const error = value instanceof Error ? value : new Error(String(value));
    return { name: error.name, message: error.message, stack: error.stack };
  } catch {
    return { name: 'Error', message: 'Unknown worker error' };
  }
};

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
  const structured = structuredHostErrorFields(value);
  if (structured) {
    // HostError's three public fields are the whole cross-seam payload. Error
    // messages, causes and stacks may contain provider URLs, tokens or paths.
    return { name: 'HostError', message: PUBLIC_HOST_ERROR_MESSAGE, ...structured };
  }
  return ordinaryWorkerError(value);
}

/** Serialize an exception from a guest-initiated host call, never diagnostics. */
export function serializeNativeHostCallError(
  // Reason: capability implementations may throw arbitrary values at the worker seam.
  value: unknown,
  expectedMethod: string
): NativeWorkerError {
  const structured = structuredHostErrorFields(value, expectedMethod);
  return structured
    ? { name: 'HostError', message: PUBLIC_HOST_ERROR_MESSAGE, ...structured }
    : { name: 'Error', message: PUBLIC_HOST_ERROR_MESSAGE };
}

export function deserializeNativeWorkerError(value: NativeWorkerError): Error {
  if (typeof value !== 'object' || value === null) return new Error('Malformed worker error');
  try {
    const keys = Object.keys(value).sort().join(',');
    const structured = structuredHostErrorFields(value);
    if (keys === 'code,message,method,name,retryable' && structured && value.name === 'HostError') {
      const error = new Error(PUBLIC_HOST_ERROR_MESSAGE);
      error.name = 'HostError';
      Object.assign(error, {
        method: structured.method,
        code: structured.code,
        retryable: structured.retryable,
      });
      return error;
    }

    if (
      (keys !== 'message,name' && keys !== 'message,name,stack') ||
      typeof value.name !== 'string' ||
      typeof value.message !== 'string' ||
      (value.stack !== undefined && typeof value.stack !== 'string')
    ) {
      return new Error('Malformed worker error');
    }
    const error = new Error(value.message);
    error.name = value.name;
    if (value.stack) error.stack = value.stack;
    return error;
  } catch {
    return new Error('Malformed worker error');
  }
}
