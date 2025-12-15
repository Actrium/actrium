import { describe, expect, it } from 'bun:test';
import React from 'react';
import * as RillReconciler from '../reconciler';
import { Engine } from './engine';

describe('Engine CALL_FUNCTION (host callback registry)', () => {
  it('不依赖 Guest __invokeCallback 也能触发回调', async () => {
    // 先通过 reconciler 初始化全局 CallbackRegistry
    const sendToHost = () => {};
    RillReconciler.render(React.createElement('View', null), sendToHost);
    const registry = RillReconciler.getCallbackRegistry(sendToHost);
    expect(registry).not.toBeNull();

    let called = false;
    const fnId = registry!.register(() => {
      called = true;
    });

    // 加载一个不注入 __invokeCallback 的最小 bundle
    const engine = new Engine({ debug: false });
    await engine.loadBundle('console.log("loaded")');

    // 如果 Engine 仍然走 sandbox eval 的 __invokeCallback，这里会找不到函数而导致 called=false
    await engine.sendToSandbox({ type: 'CALL_FUNCTION', fnId, args: [] });

    expect(called).toBe(true);

    // 清理
    registry!.remove(fnId);
    engine.destroy();
    RillReconciler.unmount(sendToHost);
  });
});
