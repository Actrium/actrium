import { describe, expect, it } from 'bun:test';
import React from 'react';
import * as RillReconciler from '@rill/let/reconciler';
import { Engine } from './engine';

describe('Engine CALL_FUNCTION (host callback registry)', () => {
  it('should invoke callback without relying on Guest __invokeCallback', async () => {
    // Initialize global CallbackRegistry via reconciler
    const sendToHost = () => {};
    RillReconciler.render(React.createElement('View', null), sendToHost);
    const registry = RillReconciler.getCallbackRegistry(sendToHost);
    expect(registry).not.toBeNull();

    let called = false;
    const fnId = registry!.register(() => {
      called = true;
    });

    // Load a minimal bundle that doesn't inject __invokeCallback
    const engine = new Engine({ debug: false });
    await engine.loadBundle('console.log("loaded")');

    // If Engine still uses sandbox eval's __invokeCallback, the function won't be found and called=false
    await engine.sendToSandbox({ type: 'CALL_FUNCTION', fnId, args: [] });

    expect(called).toBe(true);

    // Cleanup
    registry!.remove(fnId);
    engine.destroy();
    RillReconciler.unmount(sendToHost);
  });
});
