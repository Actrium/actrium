import { describe, expect, it } from 'bun:test';
import {
  deserializeNativeWorkerError,
  serializeNativeHostCallError,
  serializeNativeWorkerError,
} from '../native-worker-protocol';

describe('native worker error boundary', () => {
  it('transports only the three public HostError fields', () => {
    const wire = serializeNativeWorkerError(
      Object.assign(new Error('token=https://secret.example'), {
        method: 'store.get',
        code: 'denied',
        retryable: false,
        internalContext: 'must not cross',
      })
    );
    expect(wire).toEqual({
      name: 'HostError',
      message: 'Host API call failed',
      method: 'store.get',
      code: 'denied',
      retryable: false,
    });
    expect(wire).not.toHaveProperty('stack');
    expect(JSON.stringify(wire)).not.toContain('secret');
    const decoded = deserializeNativeWorkerError(wire);
    expect(decoded).toMatchObject({
      name: 'HostError',
      message: 'Host API call failed',
      method: 'store.get',
      code: 'denied',
      retryable: false,
    });
    expect(decoded.stack).not.toContain('secret');

    expect(
      serializeNativeHostCallError(
        Object.assign(new Error('provider URL'), {
          method: 'store.get',
          code: 'timeout',
          retryable: true,
        }),
        'store.get'
      )
    ).toEqual({
      name: 'HostError',
      message: 'Host API call failed',
      method: 'store.get',
      code: 'timeout',
      retryable: true,
    });
  });

  it('redacts non-conforming and cross-method host-call exceptions', () => {
    const secret = Object.assign(new Error('database password'), {
      method: 'store.get',
      code: 'denied',
      retryable: false,
    });
    expect(serializeNativeHostCallError(secret, 'store.put')).toEqual({
      name: 'Error',
      message: 'Host API call failed',
    });
    expect(
      serializeNativeHostCallError(
        Object.defineProperty({}, 'method', {
          get() {
            throw new Error('getter secret');
          },
        }),
        'store.put'
      )
    ).toEqual({ name: 'Error', message: 'Host API call failed' });
  });

  it('rejects extra fields and malformed codes while decoding', () => {
    const extra = deserializeNativeWorkerError({
      name: 'HostError',
      message: 'Host API call failed',
      method: 'store.get',
      code: 'denied',
      retryable: false,
      secret: 'leak',
    } as never);
    expect(extra.message).toBe('Malformed worker error');

    const invalidCode = deserializeNativeWorkerError({
      name: 'HostError',
      message: 'Host API call failed',
      method: 'store.get',
      code: 'Denied',
      retryable: false,
    });
    expect(invalidCode.message).toBe('Malformed worker error');

    const longestCode = 'a'.repeat(63);
    expect(
      serializeNativeHostCallError(
        { method: 'store.get', code: longestCode, retryable: false },
        'store.get'
      )
    ).toMatchObject({ code: longestCode });
    expect(
      deserializeNativeWorkerError({
        name: 'HostError',
        message: 'Host API call failed',
        method: 'store.get',
        code: 'a'.repeat(64),
        retryable: false,
      }).message
    ).toBe('Malformed worker error');

    const hostile = new Proxy({} as never, {
      ownKeys() {
        throw new Error('enumeration secret');
      },
    });
    expect(deserializeNativeWorkerError(hostile).message).toBe('Malformed worker error');
  });
});
