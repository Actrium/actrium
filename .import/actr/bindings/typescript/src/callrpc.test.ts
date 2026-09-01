/**
 * Tests for `ActrRef.callRpc`, the typed call that consumes a generated
 * request companion.
 *
 * The companion carries the `(actr.payload_type)` lane declared in proto as
 * a runtime-neutral number; `callRpc` must forward exactly that lane to the
 * native raw call instead of restating a hand-picked default. These tests
 * capture the native boundary with a fake `NativeActrRef` and assert what
 * actually crosses it.
 */

import { describe, it, expect } from 'vitest';
import { ActrRef, PayloadType, type RpcCompanion } from '../typescript';
import type { ActrId } from '../index';

type CapturedCall = {
  routeKey: string;
  payloadType: PayloadType;
  payload: Buffer;
  timeoutMs: number;
};

function fakeNativeRef(responseBytes: Buffer) {
  const captured: CapturedCall[] = [];
  const native = {
    call: (
      _target: ActrId,
      routeKey: string,
      payloadType: PayloadType,
      payload: Buffer,
      timeoutMs: number,
    ): Promise<Buffer> => {
      captured.push({ routeKey, payloadType, payload, timeoutMs });
      return Promise.resolve(responseBytes);
    },
  };
  return { captured, native };
}

const target = null as unknown as ActrId;

function makeCompanion(
  routeKey: string,
  payloadType: number,
): RpcCompanion<{ message: string }, { reply: string }> {
  return {
    routeKey,
    payloadType,
    encode: (message) => Buffer.from(message.message),
    response: {
      decode: (bytes) => ({ reply: Buffer.from(bytes).toString() }),
    },
  };
}

describe('ActrRef.callRpc', () => {
  it('forwards the companion routeKey and declared payloadType to the raw call', async () => {
    const responseBytes = Buffer.from('pong');
    const { captured, native } = fakeNativeRef(responseBytes);
    const ref = new ActrRef(native as never);
    const companion = makeCompanion('echo.lane.LaneService.SignalEcho', 1);

    const response = await ref.callRpc(
      target,
      companion,
      { message: 'ping' },
      5000,
    );

    expect(captured).toHaveLength(1);
    expect(captured[0].routeKey).toBe('echo.lane.LaneService.SignalEcho');
    expect(captured[0].payloadType).toBe(PayloadType.RpcSignal);
    expect(captured[0].payload.toString()).toBe('ping');
    expect(captured[0].timeoutMs).toBe(5000);
    expect(response.reply).toBe('pong');
  });

  it('forwards the fallback lane of an unannotated method unchanged', async () => {
    const { captured, native } = fakeNativeRef(Buffer.from('ok'));
    const ref = new ActrRef(native as never);
    const companion = makeCompanion('echo.lane.LaneService.PlainEcho', 0);

    await ref.callRpc(target, companion, { message: 'ping' }, 30_000);

    expect(captured[0].payloadType).toBe(PayloadType.RpcReliable);
    expect(captured[0].timeoutMs).toBe(30000);
  });
});
