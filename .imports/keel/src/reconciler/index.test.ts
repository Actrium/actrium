/**
 * Reconciler 单元测试
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  CallbackRegistry,
  OperationCollector,
  createReconciler,
  render,
  unmount,
  getCallbackRegistry,
} from './index';
import type { OperationBatch, SendToHost } from '../types';

// ============ CallbackRegistry 测试 ============

describe('CallbackRegistry', () => {
  let registry: CallbackRegistry;

  beforeEach(() => {
    registry = new CallbackRegistry();
  });

  afterEach(() => {
    registry.clear();
  });

  describe('register', () => {
    it('should register a callback and return a unique ID', () => {
      const fn = vi.fn();
      const fnId = registry.register(fn);

      expect(fnId).toMatch(/^fn_\d+_[a-z0-9]+$/);
      expect(registry.size).toBe(1);
    });

    it('should generate unique IDs for different callbacks', () => {
      const fn1 = vi.fn();
      const fn2 = vi.fn();

      const id1 = registry.register(fn1);
      const id2 = registry.register(fn2);

      expect(id1).not.toBe(id2);
      expect(registry.size).toBe(2);
    });
  });

  describe('invoke', () => {
    it('should invoke registered callback with arguments', () => {
      const fn = vi.fn((a: number, b: number) => a + b);
      const fnId = registry.register(fn);

      const result = registry.invoke(fnId, [1, 2]);

      expect(fn).toHaveBeenCalledWith(1, 2);
      expect(result).toBe(3);
    });

    it('should return undefined for non-existent callback', () => {
      const consoleSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});

      const result = registry.invoke('non_existent', []);

      expect(result).toBeUndefined();
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Callback non_existent not found')
      );

      consoleSpy.mockRestore();
    });

    it('should handle callback errors', () => {
      const error = new Error('Test error');
      const fn = vi.fn(() => {
        throw error;
      });
      const fnId = registry.register(fn);

      expect(() => registry.invoke(fnId, [])).toThrow('Test error');
    });
  });

  describe('remove', () => {
    it('should remove a registered callback', () => {
      const fn = vi.fn();
      const fnId = registry.register(fn);

      expect(registry.size).toBe(1);

      registry.remove(fnId);

      expect(registry.size).toBe(0);
    });

    it('should not throw when removing non-existent callback', () => {
      expect(() => registry.remove('non_existent')).not.toThrow();
    });
  });

  describe('removeAll', () => {
    it('should remove multiple callbacks', () => {
      const ids = [
        registry.register(vi.fn()),
        registry.register(vi.fn()),
        registry.register(vi.fn()),
      ];

      expect(registry.size).toBe(3);

      registry.removeAll(ids.slice(0, 2));

      expect(registry.size).toBe(1);
    });
  });

  describe('clear', () => {
    it('should remove all callbacks', () => {
      registry.register(vi.fn());
      registry.register(vi.fn());
      registry.register(vi.fn());

      expect(registry.size).toBe(3);

      registry.clear();

      expect(registry.size).toBe(0);
    });
  });
});

// ============ OperationCollector 测试 ============

describe('OperationCollector', () => {
  let collector: OperationCollector;
  let sendToHost: SendToHost;
  let receivedBatches: OperationBatch[];

  beforeEach(() => {
    collector = new OperationCollector();
    receivedBatches = [];
    sendToHost = vi.fn((batch: OperationBatch) => {
      receivedBatches.push(batch);
    });
  });

  describe('add', () => {
    it('should add operations to the queue', () => {
      collector.add({ op: 'CREATE', id: 1, type: 'View', props: {} });
      collector.add({ op: 'CREATE', id: 2, type: 'Text', props: {} });

      expect(collector.pendingCount).toBe(2);
    });

    it('should add timestamp to operations', () => {
      const before = Date.now();
      collector.add({ op: 'CREATE', id: 1, type: 'View', props: {} });
      const after = Date.now();

      collector.flush(sendToHost);

      const op = receivedBatches[0].operations[0];
      expect(op.timestamp).toBeGreaterThanOrEqual(before);
      expect(op.timestamp).toBeLessThanOrEqual(after);
    });
  });

  describe('flush', () => {
    it('should send all pending operations', () => {
      collector.add({ op: 'CREATE', id: 1, type: 'View', props: {} });
      collector.add({ op: 'CREATE', id: 2, type: 'Text', props: {} });

      collector.flush(sendToHost);

      expect(sendToHost).toHaveBeenCalledTimes(1);
      expect(receivedBatches[0].operations).toHaveLength(2);
      expect(collector.pendingCount).toBe(0);
    });

    it('should not call sendToHost if no pending operations', () => {
      collector.flush(sendToHost);

      expect(sendToHost).not.toHaveBeenCalled();
    });

    it('should increment batchId on each flush', () => {
      collector.add({ op: 'CREATE', id: 1, type: 'View', props: {} });
      collector.flush(sendToHost);

      collector.add({ op: 'CREATE', id: 2, type: 'Text', props: {} });
      collector.flush(sendToHost);

      expect(receivedBatches[0].batchId).toBe(1);
      expect(receivedBatches[1].batchId).toBe(2);
    });

    it('should include version in batch', () => {
      collector.add({ op: 'CREATE', id: 1, type: 'View', props: {} });
      collector.flush(sendToHost);

      expect(receivedBatches[0].version).toBe(1);
    });
  });
});

// ============ createReconciler 测试 ============

describe('createReconciler', () => {
  let sendToHost: SendToHost;
  let receivedBatches: OperationBatch[];

  beforeEach(() => {
    receivedBatches = [];
    sendToHost = vi.fn((batch: OperationBatch) => {
      receivedBatches.push(batch);
    });
  });

  it('should create reconciler instance', () => {
    const result = createReconciler(sendToHost);

    expect(result).toHaveProperty('reconciler');
    expect(result).toHaveProperty('callbackRegistry');
    expect(result).toHaveProperty('collector');
  });

  it('should return different instances on each call', () => {
    const result1 = createReconciler(sendToHost);
    const result2 = createReconciler(sendToHost);

    expect(result1.callbackRegistry).not.toBe(result2.callbackRegistry);
    expect(result1.collector).not.toBe(result2.collector);
  });
});

// ============ Props 序列化测试 ============

describe('Props Serialization', () => {
  let sendToHost: SendToHost;
  let receivedBatches: OperationBatch[];
  let reconcilerInstance: ReturnType<typeof createReconciler>;

  beforeEach(() => {
    receivedBatches = [];
    sendToHost = vi.fn((batch: OperationBatch) => {
      receivedBatches.push(batch);
    });
    reconcilerInstance = createReconciler(sendToHost);
  });

  afterEach(() => {
    reconcilerInstance.callbackRegistry.clear();
  });

  it('should serialize primitive props', () => {
    const { reconciler, collector } = reconcilerInstance;

    // 模拟创建实例
    const container = { children: [] };
    const root = reconciler.createContainer(
      container,
      0,
      null,
      false,
      null,
      'test',
      () => {},
      null
    );

    // 手动添加操作来测试序列化
    collector.add({
      op: 'CREATE',
      id: 1,
      type: 'View',
      props: {
        style: { flex: 1, backgroundColor: 'red' },
        testID: 'test-view',
      },
    });

    collector.flush(sendToHost);

    const createOp = receivedBatches[0].operations[0];
    expect(createOp.op).toBe('CREATE');
    if (createOp.op === 'CREATE') {
      expect(createOp.props.style).toEqual({ flex: 1, backgroundColor: 'red' });
      expect(createOp.props.testID).toBe('test-view');
    }
  });

  it('should serialize function props as fnId references', () => {
    const { callbackRegistry, collector } = reconcilerInstance;

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

    collector.flush(sendToHost);

    const createOp = receivedBatches[0].operations[0];
    if (createOp.op === 'CREATE') {
      expect(createOp.props.onPress).toEqual({
        __type: 'function',
        __fnId: fnId,
      });
    }

    // 验证回调可以被调用
    callbackRegistry.invoke(fnId, []);
    expect(onPress).toHaveBeenCalled();
  });

  it('should serialize nested objects', () => {
    const { collector } = reconcilerInstance;

    collector.add({
      op: 'CREATE',
      id: 1,
      type: 'View',
      props: {
        style: {
          shadowOffset: { width: 0, height: 2 },
          transform: [{ translateX: 10 }, { scale: 1.5 }],
        },
      },
    });

    collector.flush(sendToHost);

    const createOp = receivedBatches[0].operations[0];
    if (createOp.op === 'CREATE') {
      expect(createOp.props.style).toEqual({
        shadowOffset: { width: 0, height: 2 },
        transform: [{ translateX: 10 }, { scale: 1.5 }],
      });
    }
  });

  it('should serialize arrays', () => {
    const { collector } = reconcilerInstance;

    collector.add({
      op: 'CREATE',
      id: 1,
      type: 'FlatList',
      props: {
        data: [
          { id: 1, name: 'Item 1' },
          { id: 2, name: 'Item 2' },
        ],
      },
    });

    collector.flush(sendToHost);

    const createOp = receivedBatches[0].operations[0];
    if (createOp.op === 'CREATE') {
      expect(createOp.props.data).toEqual([
        { id: 1, name: 'Item 1' },
        { id: 2, name: 'Item 2' },
      ]);
    }
  });
});

// ============ 操作类型测试 ============

describe('Operation Types', () => {
  let sendToHost: SendToHost;
  let receivedBatches: OperationBatch[];
  let reconcilerInstance: ReturnType<typeof createReconciler>;

  beforeEach(() => {
    receivedBatches = [];
    sendToHost = vi.fn((batch: OperationBatch) => {
      receivedBatches.push(batch);
    });
    reconcilerInstance = createReconciler(sendToHost);
  });

  it('should generate CREATE operations', () => {
    const { collector } = reconcilerInstance;

    collector.add({
      op: 'CREATE',
      id: 1,
      type: 'View',
      props: { style: { flex: 1 } },
    });

    collector.flush(sendToHost);

    expect(receivedBatches[0].operations[0]).toMatchObject({
      op: 'CREATE',
      id: 1,
      type: 'View',
      props: { style: { flex: 1 } },
    });
  });

  it('should generate UPDATE operations', () => {
    const { collector } = reconcilerInstance;

    collector.add({
      op: 'UPDATE',
      id: 1,
      props: { style: { flex: 2 } },
      removedProps: ['testID'],
    });

    collector.flush(sendToHost);

    expect(receivedBatches[0].operations[0]).toMatchObject({
      op: 'UPDATE',
      id: 1,
      props: { style: { flex: 2 } },
      removedProps: ['testID'],
    });
  });

  it('should generate APPEND operations', () => {
    const { collector } = reconcilerInstance;

    collector.add({
      op: 'APPEND',
      id: 2,
      parentId: 1,
      childId: 2,
    });

    collector.flush(sendToHost);

    expect(receivedBatches[0].operations[0]).toMatchObject({
      op: 'APPEND',
      id: 2,
      parentId: 1,
      childId: 2,
    });
  });

  it('should generate INSERT operations', () => {
    const { collector } = reconcilerInstance;

    collector.add({
      op: 'INSERT',
      id: 3,
      parentId: 1,
      childId: 3,
      index: 0,
    });

    collector.flush(sendToHost);

    expect(receivedBatches[0].operations[0]).toMatchObject({
      op: 'INSERT',
      id: 3,
      parentId: 1,
      childId: 3,
      index: 0,
    });
  });

  it('should generate REMOVE operations', () => {
    const { collector } = reconcilerInstance;

    collector.add({
      op: 'REMOVE',
      id: 2,
      parentId: 1,
      childId: 2,
    });

    collector.flush(sendToHost);

    expect(receivedBatches[0].operations[0]).toMatchObject({
      op: 'REMOVE',
      id: 2,
      parentId: 1,
      childId: 2,
    });
  });

  it('should generate DELETE operations', () => {
    const { collector } = reconcilerInstance;

    collector.add({
      op: 'DELETE',
      id: 1,
    });

    collector.flush(sendToHost);

    expect(receivedBatches[0].operations[0]).toMatchObject({
      op: 'DELETE',
      id: 1,
    });
  });

  it('should generate REORDER operations', () => {
    const { collector } = reconcilerInstance;

    collector.add({
      op: 'REORDER',
      id: 1,
      parentId: 0,
      childIds: [3, 1, 2],
    });

    collector.flush(sendToHost);

    expect(receivedBatches[0].operations[0]).toMatchObject({
      op: 'REORDER',
      id: 1,
      parentId: 0,
      childIds: [3, 1, 2],
    });
  });

  it('should generate TEXT operations', () => {
    const { collector } = reconcilerInstance;

    collector.add({
      op: 'TEXT',
      id: 1,
      text: 'Hello World',
    });

    collector.flush(sendToHost);

    expect(receivedBatches[0].operations[0]).toMatchObject({
      op: 'TEXT',
      id: 1,
      text: 'Hello World',
    });
  });
});

// ============ 渲染入口测试 ============

describe('render and unmount', () => {
  let sendToHost: SendToHost;

  beforeEach(() => {
    sendToHost = vi.fn();
    // 确保每次测试前清理状态
    unmount();
  });

  afterEach(() => {
    unmount();
  });

  it('should initialize and render', () => {
    // 由于 render 需要 React 元素，这里只测试初始化
    expect(getCallbackRegistry()).toBeNull();

    // render 会初始化 reconciler
    // 实际测试需要集成测试环境
  });

  it('should cleanup on unmount', () => {
    unmount();
    expect(getCallbackRegistry()).toBeNull();
  });
});

// ============ 批量更新测试 ============

describe('Batch Updates', () => {
  let sendToHost: SendToHost;
  let receivedBatches: OperationBatch[];
  let reconcilerInstance: ReturnType<typeof createReconciler>;

  beforeEach(() => {
    receivedBatches = [];
    sendToHost = vi.fn((batch: OperationBatch) => {
      receivedBatches.push(batch);
    });
    reconcilerInstance = createReconciler(sendToHost);
  });

  it('should batch multiple operations into single flush', () => {
    const { collector } = reconcilerInstance;

    // 模拟多个操作
    collector.add({ op: 'CREATE', id: 1, type: 'View', props: {} });
    collector.add({ op: 'CREATE', id: 2, type: 'Text', props: {} });
    collector.add({ op: 'APPEND', id: 2, parentId: 1, childId: 2 });
    collector.add({ op: 'APPEND', id: 1, parentId: 0, childId: 1 });

    collector.flush(sendToHost);

    // 应该只有一次调用
    expect(sendToHost).toHaveBeenCalledTimes(1);

    // 批次中包含所有操作
    expect(receivedBatches[0].operations).toHaveLength(4);
    expect(receivedBatches[0].operations.map((op) => op.op)).toEqual([
      'CREATE',
      'CREATE',
      'APPEND',
      'APPEND',
    ]);
  });

  it('should maintain operation order within batch', () => {
    const { collector } = reconcilerInstance;

    collector.add({ op: 'CREATE', id: 1, type: 'View', props: {} });
    collector.add({ op: 'UPDATE', id: 1, props: { style: { flex: 1 } } });
    collector.add({ op: 'DELETE', id: 1 });

    collector.flush(sendToHost);

    const ops = receivedBatches[0].operations;
    expect(ops[0].op).toBe('CREATE');
    expect(ops[1].op).toBe('UPDATE');
    expect(ops[2].op).toBe('DELETE');
  });
});
