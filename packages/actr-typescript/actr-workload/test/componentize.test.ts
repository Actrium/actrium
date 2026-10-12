import { describe, expect, it } from 'vitest';
import { runInNewContext } from 'node:vm';

import {
  ASYNC_COMPONENTIZE_UPSTREAM_ISSUE,
  explainComponentizeFailure,
  shimSource,
} from '../src/componentize.js';

describe('componentize shim', () => {
  it('threads invocation context into user dispatch', () => {
    const source = shimSource('/tmp/workload.js', '/tmp/runtime.js');

    expect(source).toMatch(
      /activeWorkload\(\)\.dispatch\([\s\S]*?requestId:[\s\S]*?}, ctx\)/,
    );
    expect(source).toContain('timeoutMs: Number(envelope?.timeoutMs ?? 0)');
  });

  it('forwards structured error events unchanged', () => {
    const source = shimSource('/tmp/workload.js', '/tmp/runtime.js');

    expect(source).toContain('activeWorkload().onError?.(event, ctx)');
    expect(source).not.toContain('errorMessage(event)');
  });

  it('threads invocation context into registered stream callbacks', () => {
    const source = shimSource('/tmp/workload.js', '/tmp/runtime.js');

    expect(source).toContain('__dispatchDataChunk(chunk, sender, ctx)');
  });

  it('feeds stream subscriptions even when a legacy onDataChunk hook exists', async () => {
    const source = shimSource('/tmp/workload.js', '/tmp/runtime.js')
      .replace(/^\s*import .*;$/gm, '')
      .replace('export const workload =', 'globalThis.workload =');
    const ctx = { ctxToken: 7n };
    const chunk = { streamId: 's', sequence: 1n };
    const sender = { serialNumber: 1n };
    const hookCalls: unknown[][] = [];
    const dispatched: unknown[][] = [];
    const sandbox = {
      userWorkload: {
        dispatch: () => new Uint8Array(),
        onDataChunk: async (...args: unknown[]) => {
          hookCalls.push(args);
        },
      },
      withInvocationCtx: async (_c: unknown, body: () => Promise<void>) =>
        body(),
      __dispatchDataChunk: async (...args: unknown[]) => {
        dispatched.push(args);
      },
      __hasStreamSubscription: () => true,
      workload: {} as Record<string, (...args: unknown[]) => Promise<void>>,
    };
    runInNewContext(source, sandbox);
    await sandbox.workload.onDataChunk(chunk, sender, ctx);
    expect(dispatched).toHaveLength(1);
    expect(hookCalls).toHaveLength(1);
  });

  it('keeps legacy-hook-only routing when no subscription exists', async () => {
    const source = shimSource('/tmp/workload.js', '/tmp/runtime.js')
      .replace(/^\s*import .*;$/gm, '')
      .replace('export const workload =', 'globalThis.workload =');
    const ctx = { ctxToken: 7n };
    const chunk = { streamId: 's', sequence: 1n };
    const sender = { serialNumber: 1n };
    const hookCalls: unknown[][] = [];
    const sandbox = {
      userWorkload: {
        dispatch: () => new Uint8Array(),
        onDataChunk: async (...args: unknown[]) => {
          hookCalls.push(args);
        },
      },
      withInvocationCtx: async (_c: unknown, body: () => Promise<void>) =>
        body(),
      __dispatchDataChunk: async () => {
        throw new Error('No stream subscription for s');
      },
      __hasStreamSubscription: () => false,
      workload: {} as Record<string, (...args: unknown[]) => Promise<void>>,
    };
    runInNewContext(source, sandbox);
    await sandbox.workload.onDataChunk(chunk, sender, ctx);
    // No subscription: the chunk never touches the queue machinery, so the
    // legacy hook keeps working exactly as before.
    expect(hookCalls).toHaveLength(1);
  });

  it('surfaces stale deliveries when neither a subscription nor a hook exists', async () => {
    const source = shimSource('/tmp/workload.js', '/tmp/runtime.js')
      .replace(/^\s*import .*;$/gm, '')
      .replace('export const workload =', 'globalThis.workload =');
    const ctx = { ctxToken: 7n };
    const chunk = { streamId: 's', sequence: 1n };
    const sender = { serialNumber: 1n };
    const sandbox = {
      userWorkload: { dispatch: () => new Uint8Array() },
      withInvocationCtx: async (_c: unknown, body: () => Promise<void>) =>
        body(),
      __dispatchDataChunk: async () => {
        throw new Error('No stream subscription for s');
      },
      __hasStreamSubscription: () => false,
      workload: {} as Record<string, (...args: unknown[]) => Promise<void>>,
    };
    runInNewContext(source, sandbox);
    await expect(
      sandbox.workload.onDataChunk(chunk, sender, ctx),
    ).rejects.toThrow('No stream subscription for s');
  });

  it('delivers every observation hook with the original event and context', async () => {
    const signaling = [
      'onSignalingConnecting',
      'onSignalingConnected',
      'onSignalingDisconnected',
    ];
    const events = [
      'onWebsocketConnecting',
      'onWebsocketConnected',
      'onWebsocketDisconnected',
      'onWebrtcConnecting',
      'onWebrtcConnected',
      'onWebrtcDisconnected',
      'onWebrtcStateChanged',
      'onCredentialRenewed',
      'onCredentialExpiring',
      'onMailboxBackpressure',
    ];
    const calls: { name: string; args: unknown[] }[] = [];
    const userWorkload = {
      dispatch: () => new Uint8Array(),
      ...Object.fromEntries(
        [...signaling, ...events].map((name) => [
          name,
          async (...args: unknown[]) => {
            await Promise.resolve();
            calls.push({ name, args });
          },
        ]),
      ),
    };
    const source = shimSource('/tmp/workload.js', '/tmp/runtime.js')
      .replace(/^\s*import .*;$/gm, '')
      .replace('export const workload =', 'globalThis.workload =');
    const ctx = { ctxToken: 7n };
    const event = { revision: 9n };
    const sandbox = {
      userWorkload,
      withInvocationCtx: async (actual: unknown, body: () => Promise<void>) => {
        expect(actual).toBe(ctx);
        await body();
      },
      workload: {} as Record<string, (...args: unknown[]) => Promise<void>>,
    };
    runInNewContext(source, sandbox);
    for (const name of [...signaling, ...events]) {
      const args = signaling.includes(name) ? [ctx] : [event, ctx];
      await sandbox.workload[name](...args);
      const call = calls.at(-1)!;
      expect(call.name).toBe(name);
      expect(call.args).toHaveLength(args.length);
      args.forEach((arg, index) => expect(call.args[index]).toBe(arg));
    }
    expect(calls).toHaveLength(signaling.length + events.length);
    // Observation hooks remain optional on user workloads.
    sandbox.userWorkload = { dispatch: () => new Uint8Array() };
    for (const name of [...signaling, ...events]) {
      await sandbox.workload[name](
        ...(signaling.includes(name) ? [ctx] : [event, ctx]),
      );
    }
  });

  it('explains the known ComponentizeJS async-func blocker', () => {
    const error = explainComponentizeFailure(
      new Error(
        'spidermonkey-embedding-splicer/src/bindgen.rs: not yet implemented',
      ),
    );

    expect(error.message).toContain('missing async-func export support');
    expect(error.message).toContain(ASYNC_COMPONENTIZE_UPSTREAM_ISSUE);
  });

  it('does not mask unrelated componentization failures', () => {
    const original = new Error('unrelated jco failure');

    expect(explainComponentizeFailure(original)).toBe(original);
  });
});
