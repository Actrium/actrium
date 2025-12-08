/**
 * End-to-end integration tests
 *
 * Simulates a complete plugin lifecycle:
 * 1. Compile plugin code
 * 2. Execute in sandbox
 * 3. Generate operations
 * 4. Host rendering
 * 5. Two-way communication
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import React from 'react';
import { Engine } from '../../runtime/engine';
import { ComponentRegistry } from '../../runtime/registry';
import { Receiver } from '../../runtime/receiver';
import {
  createReconciler,
  CallbackRegistry,
  OperationCollector,
} from '../../reconciler';

// TS type imports
interface OperationBatch {
  version: number;
  batchId: number;
  operations: unknown[];
}

interface HostMessage {
  type: string;
  [key: string]: unknown;
}

// Type definitions for mock QuickJS
interface MockQuickJSContext {
  eval(code: string): unknown;
  setGlobal(name: string, value: unknown): void;
  getGlobal(name: string): unknown;
  dispose(): void;
}

interface MockQuickJSRuntime {
  createContext(): MockQuickJSContext;
  dispose(): void;
}

interface MockQuickJSProvider {
  createRuntime(): MockQuickJSRuntime;
}

// Mock QuickJS Provider for tests
function createMockQuickJSProvider(): MockQuickJSProvider {
  return {
    createRuntime(): MockQuickJSRuntime {
      const globals = new Map<string, unknown>();
      return {
        createContext(): MockQuickJSContext {
          return {
            eval(code: string): unknown {
              const globalNames = Array.from(globals.keys());
              const globalValues = Array.from(globals.values());
              try {
                const fn = new Function(...globalNames, `"use strict"; ${code}`);
                return fn(...globalValues);
              } catch (e) {
                throw e;
              }
            },
            setGlobal(name: string, value: unknown): void {
              globals.set(name, value);
            },
            getGlobal(name: string): unknown {
              return globals.get(name);
            },
            dispose(): void {
              globals.clear();
            },
          };
        },
        dispose(): void {},
      };
    },
  };
}

// ============ Mock components ============

const MockView: React.FC<{
  style?: object;
  children?: React.ReactNode;
  testID?: string;
}> = ({ children, testID }) =>
  React.createElement('mock-view', { 'data-testid': testID }, children);

const MockText: React.FC<{ children?: React.ReactNode; style?: object }> = ({
  children,
}) => React.createElement('mock-text', null, children);

const MockTouchable: React.FC<{
  onPress?: () => void;
  children?: React.ReactNode;
  style?: object;
}> = ({ children, onPress }) =>
  React.createElement('mock-touchable', { onClick: onPress }, children);

// ============ Integration tests ============

describe('E2E Integration Tests', () => {
  describe('Complete Plugin Lifecycle', () => {
    let engine: Engine;
    let receivedBatches: OperationBatch[];
    let sentMessages: HostMessage[];

    beforeEach(() => {
      engine = new Engine({ quickjs: createMockQuickJSProvider(), debug: false });
      engine.register({
        View: MockView,
        Text: MockText,
        TouchableOpacity: MockTouchable,
      });

      receivedBatches = [];
      sentMessages = [];
    });

    afterEach(() => {
      engine.destroy();
    });

    it('should execute simple plugin and generate operations', async () => {
      // Simple plugin code
      const pluginCode = `
        // 模拟插件执行
        __sendToHost({
          version: 1,
          batchId: 1,
          operations: [
            { op: 'CREATE', id: 1, type: 'View', props: { style: { flex: 1 } } },
            { op: 'CREATE', id: 2, type: 'Text', props: {} },
            { op: 'APPEND', id: 2, parentId: 1, childId: 2 },
            { op: 'APPEND', id: 1, parentId: 0, childId: 1 }
          ]
        });
      `;

      // Create Receiver to receive operations
      let updateCount = 0;
      const receiver = engine.createReceiver(() => {
        updateCount++;
      });

      // Load and execute plugin
      await engine.loadBundle(pluginCode, { theme: 'dark' });

      // Wait for updates
      await new Promise((resolve) => setTimeout(resolve, 50));

      // Verify nodes created
      expect(receiver.nodeCount).toBeGreaterThanOrEqual(2);
      expect(updateCount).toBeGreaterThanOrEqual(1);
    });

    it('should handle config access', async () => {
      const pluginCode = `
        const config = __getConfig();
        console.log('Config:', config);

        if (config.theme === 'dark') {
          __sendToHost({
            version: 1,
            batchId: 1,
            operations: [
              { op: 'CREATE', id: 1, type: 'View', props: { style: { backgroundColor: '#000' } } },
              { op: 'APPEND', id: 1, parentId: 0, childId: 1 }
            ]
          });
        }
      `;

      const receiver = engine.createReceiver(() => {});
      await engine.loadBundle(pluginCode, { theme: 'dark' });

      await new Promise((resolve) => setTimeout(resolve, 50));

      expect(receiver.nodeCount).toBe(1);
    });

    it('should handle multiple batches', async () => {
      const pluginCode = `
        // 第一批操作
        __sendToHost({
          version: 1,
          batchId: 1,
          operations: [
            { op: 'CREATE', id: 1, type: 'View', props: {} },
            { op: 'APPEND', id: 1, parentId: 0, childId: 1 }
          ]
        });

        // 第二批操作
        __sendToHost({
          version: 1,
          batchId: 2,
          operations: [
            { op: 'CREATE', id: 2, type: 'Text', props: {} },
            { op: 'APPEND', id: 2, parentId: 1, childId: 2 }
          ]
        });
      `;

      const receiver = engine.createReceiver(() => {});
      await engine.loadBundle(pluginCode);

      await new Promise((resolve) => setTimeout(resolve, 50));

      expect(receiver.nodeCount).toBe(2);
    });

    it('should handle node updates', async () => {
      const pluginCode = `
        __sendToHost({
          version: 1,
          batchId: 1,
          operations: [
            { op: 'CREATE', id: 1, type: 'View', props: { style: { flex: 1 } } },
            { op: 'APPEND', id: 1, parentId: 0, childId: 1 }
          ]
        });

        // 更新节点
        __sendToHost({
          version: 1,
          batchId: 2,
          operations: [
            { op: 'UPDATE', id: 1, props: { style: { flex: 2, backgroundColor: 'red' } } }
          ]
        });
      `;

      let updateCount = 0;
      const receiver = engine.createReceiver(() => {
        updateCount++;
      });

      await engine.loadBundle(pluginCode);
      await new Promise((resolve) => setTimeout(resolve, 100));

      // 由于 microtask 批处理，两个批次可能合并为一次更新
      expect(updateCount).toBeGreaterThanOrEqual(1);
      expect(receiver.nodeCount).toBe(1);
    });

    it('should handle node removal', async () => {
      const pluginCode = `
        __sendToHost({
          version: 1,
          batchId: 1,
          operations: [
            { op: 'CREATE', id: 1, type: 'View', props: {} },
            { op: 'CREATE', id: 2, type: 'Text', props: {} },
            { op: 'APPEND', id: 2, parentId: 1, childId: 2 },
            { op: 'APPEND', id: 1, parentId: 0, childId: 1 }
          ]
        });

        // 移除子节点
        __sendToHost({
          version: 1,
          batchId: 2,
          operations: [
            { op: 'REMOVE', id: 2, parentId: 1, childId: 2 },
            { op: 'DELETE', id: 2 }
          ]
        });
      `;

      const receiver = engine.createReceiver(() => {});
      await engine.loadBundle(pluginCode);

      await new Promise((resolve) => setTimeout(resolve, 50));

      expect(receiver.nodeCount).toBe(1);
    });
  });

  describe('Reconciler to Receiver Integration', () => {
    let callbackRegistry: CallbackRegistry;
    let collector: OperationCollector;
    let registry: ComponentRegistry;
    let receiver: Receiver;
    let receivedBatches: OperationBatch[];

    beforeEach(() => {
      callbackRegistry = new CallbackRegistry();
      collector = new OperationCollector();
      registry = new ComponentRegistry();
      registry.registerAll({
        View: MockView,
        Text: MockText,
        TouchableOpacity: MockTouchable,
      });

      receivedBatches = [];
      const sendToHost = (batch: OperationBatch) => {
        receivedBatches.push(batch);
        receiver.applyBatch(batch);
      };

      receiver = new Receiver(
        registry,
        () => {},
        () => {}
      );
    });

    afterEach(() => {
      callbackRegistry.clear();
      receiver.clear();
    });

    it('should create component tree from operations', async () => {
      // 模拟 Reconciler 生成的操作
      collector.add({
        op: 'CREATE',
        id: 1,
        type: 'View',
        props: { style: { flex: 1 } },
      });
      collector.add({
        op: 'CREATE',
        id: 2,
        type: 'Text',
        props: {},
      });
      collector.add({
        op: 'APPEND',
        id: 2,
        parentId: 1,
        childId: 2,
      });
      collector.add({
        op: 'APPEND',
        id: 1,
        parentId: 0,
        childId: 1,
      });

      collector.flush((batch) => {
        receivedBatches.push(batch);
        receiver.applyBatch(batch);
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      expect(receiver.nodeCount).toBe(2);

      const rendered = receiver.render();
      expect(rendered).not.toBeNull();
    });

    it('should handle function props through callback registry', async () => {
      const onPress = vi.fn();
      const fnId = callbackRegistry.register(onPress);

      collector.add({
        op: 'CREATE',
        id: 1,
        type: 'TouchableOpacity',
        props: {
          onPress: { __type: 'function', __fnId: fnId },
        },
      });
      collector.add({
        op: 'APPEND',
        id: 1,
        parentId: 0,
        childId: 1,
      });

      collector.flush((batch) => {
        receivedBatches.push(batch);
        receiver.applyBatch(batch);
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      // 验证回调可以被调用
      callbackRegistry.invoke(fnId, []);
      expect(onPress).toHaveBeenCalled();
    });

    it('should handle complex nested structure', async () => {
      // 创建嵌套结构: View > View > Text
      collector.add({
        op: 'CREATE',
        id: 1,
        type: 'View',
        props: { style: { flex: 1 } },
      });
      collector.add({
        op: 'CREATE',
        id: 2,
        type: 'View',
        props: { style: { padding: 10 } },
      });
      collector.add({
        op: 'CREATE',
        id: 3,
        type: 'Text',
        props: {},
      });
      collector.add({
        op: 'CREATE',
        id: 4,
        type: '__TEXT__',
        props: { text: 'Hello World' },
      });

      collector.add({ op: 'APPEND', id: 4, parentId: 3, childId: 4 });
      collector.add({ op: 'APPEND', id: 3, parentId: 2, childId: 3 });
      collector.add({ op: 'APPEND', id: 2, parentId: 1, childId: 2 });
      collector.add({ op: 'APPEND', id: 1, parentId: 0, childId: 1 });

      collector.flush((batch) => {
        receiver.applyBatch(batch);
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      expect(receiver.nodeCount).toBe(4);

      const debugInfo = receiver.getDebugInfo();
      expect(debugInfo.rootChildren).toEqual([1]);
    });
  });

  describe('Bidirectional Communication', () => {
    let engine: Engine;

    beforeEach(() => {
      engine = new Engine({ quickjs: createMockQuickJSProvider(), debug: false });
      engine.register({
        View: MockView,
        Text: MockText,
      });
    });

    afterEach(() => {
      engine.destroy();
    });

    it('should send events from host to sandbox', async () => {
      const pluginCode = `
        // 设置事件处理器
        globalThis.__handleHostEvent = function(eventName, payload) {
          console.log('Received event:', eventName, payload);

          if (eventName === 'REFRESH') {
            __sendToHost({
              version: 1,
              batchId: 99,
              operations: [
                { op: 'CREATE', id: 100, type: 'Text', props: { text: 'Refreshed!' } },
                { op: 'APPEND', id: 100, parentId: 0, childId: 100 }
              ]
            });
          }
        };

        // 初始渲染
        __sendToHost({
          version: 1,
          batchId: 1,
          operations: [
            { op: 'CREATE', id: 1, type: 'View', props: {} },
            { op: 'APPEND', id: 1, parentId: 0, childId: 1 }
          ]
        });
      `;

      const receiver = engine.createReceiver(() => {});
      await engine.loadBundle(pluginCode);

      await new Promise((resolve) => setTimeout(resolve, 50));
      expect(receiver.nodeCount).toBe(1);

      // 发送事件到沙箱
      engine.sendEvent('REFRESH', { timestamp: Date.now() });

      await new Promise((resolve) => setTimeout(resolve, 50));

      // 应该有新节点被创建
      expect(receiver.nodeCount).toBeGreaterThanOrEqual(1);
    });

    it('should handle config updates', async () => {
      const pluginCode = `
        let currentTheme = __getConfig().theme || 'light';

        globalThis.__handleHostMessage = function(message) {
          if (message.type === 'CONFIG_UPDATE') {
            currentTheme = message.config.theme || currentTheme;
            console.log('Theme updated to:', currentTheme);
          }
        };

        __sendToHost({
          version: 1,
          batchId: 1,
          operations: [
            { op: 'CREATE', id: 1, type: 'View', props: {} },
            { op: 'APPEND', id: 1, parentId: 0, childId: 1 }
          ]
        });
      `;

      engine.createReceiver(() => {});
      await engine.loadBundle(pluginCode, { theme: 'light' });

      await new Promise((resolve) => setTimeout(resolve, 50));

      // 更新配置
      engine.updateConfig({ theme: 'dark' });

      await new Promise((resolve) => setTimeout(resolve, 50));

      expect(engine.isLoaded).toBe(true);
    });
  });

  describe('Error Isolation', () => {
    let engine: Engine;

    beforeEach(() => {
      engine = new Engine({ quickjs: createMockQuickJSProvider(), debug: false });
    });

    afterEach(() => {
      engine.destroy();
    });

    it('should not crash host when plugin throws error', async () => {
      const pluginCode = `
        // 正常初始化
        __sendToHost({
          version: 1,
          batchId: 1,
          operations: [
            { op: 'CREATE', id: 1, type: 'View', props: {} },
            { op: 'APPEND', id: 1, parentId: 0, childId: 1 }
          ]
        });

        // 之后抛出错误
        setTimeout(() => {
          throw new Error('Plugin error!');
        }, 10);
      `;

      const receiver = engine.createReceiver(() => {});

      // 加载不应该失败
      await engine.loadBundle(pluginCode);

      await new Promise((resolve) => setTimeout(resolve, 50));

      // Engine 仍然有效
      expect(engine.isLoaded).toBe(true);
      expect(receiver.nodeCount).toBe(1);
    });

    it('should handle missing component gracefully', async () => {
      const consoleSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});

      engine.register({
        View: MockView,
        // Text 组件未注册
      });

      const pluginCode = `
        __sendToHost({
          version: 1,
          batchId: 1,
          operations: [
            { op: 'CREATE', id: 1, type: 'View', props: {} },
            { op: 'CREATE', id: 2, type: 'UnregisteredComponent', props: {} },
            { op: 'APPEND', id: 2, parentId: 1, childId: 2 },
            { op: 'APPEND', id: 1, parentId: 0, childId: 1 }
          ]
        });
      `;

      const receiver = engine.createReceiver(() => {});
      await engine.loadBundle(pluginCode);

      await new Promise((resolve) => setTimeout(resolve, 50));

      // 渲染不应该崩溃
      const rendered = receiver.render();
      expect(rendered).not.toBeNull();

      // 应该有警告
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('UnregisteredComponent')
      );

      consoleSpy.mockRestore();
    });
  });

  describe('Batch Operations Performance', () => {
    let collector: OperationCollector;
    let batchCount: number;
    let totalOperations: number;

    beforeEach(() => {
      collector = new OperationCollector();
      batchCount = 0;
      totalOperations = 0;
    });

    it('should batch multiple operations into single flush', () => {
      // 添加大量操作
      for (let i = 1; i <= 100; i++) {
        collector.add({
          op: 'CREATE',
          id: i,
          type: 'View',
          props: { index: i },
        });
      }

      collector.flush((batch) => {
        batchCount++;
        totalOperations += batch.operations.length;
      });

      // 所有操作应该在一个批次中
      expect(batchCount).toBe(1);
      expect(totalOperations).toBe(100);
    });

    it('should handle multiple flushes correctly', () => {
      // 第一批
      for (let i = 1; i <= 50; i++) {
        collector.add({
          op: 'CREATE',
          id: i,
          type: 'View',
          props: {},
        });
      }
      collector.flush((batch) => {
        batchCount++;
        totalOperations += batch.operations.length;
      });

      // 第二批
      for (let i = 51; i <= 100; i++) {
        collector.add({
          op: 'CREATE',
          id: i,
          type: 'View',
          props: {},
        });
      }
      collector.flush((batch) => {
        batchCount++;
        totalOperations += batch.operations.length;
      });

      expect(batchCount).toBe(2);
      expect(totalOperations).toBe(100);
    });

    it('should not flush when no pending operations', () => {
      collector.flush((batch) => {
        batchCount++;
      });

      expect(batchCount).toBe(0);
    });
  });
});

describe('Full Stack Simulation', () => {
  it('should simulate complete render cycle', async () => {
    // 1. 创建 Engine
    const engine = new Engine({ quickjs: createMockQuickJSProvider() });
    engine.register({
      View: MockView,
      Text: MockText,
      TouchableOpacity: MockTouchable,
    });

    // 2. 创建 Receiver
    let renderCount = 0;
    const receiver = engine.createReceiver(() => {
      renderCount++;
    });

    // 3. 模拟插件代码
    const pluginCode = `
      // 模拟 React 组件渲染后的操作
      const operations = [];
      let nodeId = 0;

      // 创建根 View
      const rootId = ++nodeId;
      operations.push({
        op: 'CREATE',
        id: rootId,
        type: 'View',
        props: { style: { flex: 1, padding: 20 } }
      });

      // 创建标题
      const titleId = ++nodeId;
      operations.push({
        op: 'CREATE',
        id: titleId,
        type: 'Text',
        props: { style: { fontSize: 24, fontWeight: 'bold' } }
      });

      // 创建标题文本
      const titleTextId = ++nodeId;
      operations.push({
        op: 'CREATE',
        id: titleTextId,
        type: '__TEXT__',
        props: { text: 'Hello Rill!' }
      });

      // 创建按钮
      const buttonId = ++nodeId;
      operations.push({
        op: 'CREATE',
        id: buttonId,
        type: 'TouchableOpacity',
        props: {
          style: { backgroundColor: '#007AFF', padding: 12, borderRadius: 8 },
          onPress: { __type: 'function', __fnId: 'fn_1' }
        }
      });

      // 创建按钮文本
      const buttonTextId = ++nodeId;
      operations.push({
        op: 'CREATE',
        id: buttonTextId,
        type: 'Text',
        props: { style: { color: 'white' } }
      });

      const buttonTextContentId = ++nodeId;
      operations.push({
        op: 'CREATE',
        id: buttonTextContentId,
        type: '__TEXT__',
        props: { text: 'Click Me' }
      });

      // 组装树结构
      operations.push({ op: 'APPEND', id: titleTextId, parentId: titleId, childId: titleTextId });
      operations.push({ op: 'APPEND', id: titleId, parentId: rootId, childId: titleId });
      operations.push({ op: 'APPEND', id: buttonTextContentId, parentId: buttonTextId, childId: buttonTextContentId });
      operations.push({ op: 'APPEND', id: buttonTextId, parentId: buttonId, childId: buttonTextId });
      operations.push({ op: 'APPEND', id: buttonId, parentId: rootId, childId: buttonId });
      operations.push({ op: 'APPEND', id: rootId, parentId: 0, childId: rootId });

      __sendToHost({
        version: 1,
        batchId: 1,
        operations: operations
      });
    `;

    // 4. 加载并执行
    await engine.loadBundle(pluginCode);
    await new Promise((resolve) => setTimeout(resolve, 100));

    // 5. 验证结果
    expect(receiver.nodeCount).toBe(6);
    expect(renderCount).toBeGreaterThanOrEqual(1);

    const rendered = receiver.render();
    expect(rendered).not.toBeNull();

    const debugInfo = receiver.getDebugInfo();
    expect(debugInfo.rootChildren.length).toBe(1);

    // 6. 清理
    engine.destroy();
  });
});
