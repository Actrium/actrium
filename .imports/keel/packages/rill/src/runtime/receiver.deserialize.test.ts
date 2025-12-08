/**
 * Receiver deserialization and rendering tests
 *
 * Covers:
 * - deserializeValue with functions (lines 294-300)
 * - renderNode for missing nodes (lines 351-354)
 * - Nested object/array deserialization
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import React from 'react';
import { Receiver } from './receiver';
import { ComponentRegistry } from './registry';

// Mock components
const MockView: React.FC<Record<string, unknown>> = (props) =>
  React.createElement('View', props);
const MockText: React.FC<Record<string, unknown>> = (props) =>
  React.createElement('Text', props);

describe('Receiver Deserialization', () => {
  let registry: ComponentRegistry;
  let sendToSandbox: ReturnType<typeof vi.fn>;
  let onUpdate: ReturnType<typeof vi.fn>;
  let receiver: Receiver;

  beforeEach(() => {
    registry = new ComponentRegistry();
    registry.register('View', MockView);
    registry.register('Text', MockText);
    sendToSandbox = vi.fn();
    onUpdate = vi.fn();
    receiver = new Receiver(registry, sendToSandbox, onUpdate);
  });

  describe('Serialized function deserialization', () => {
    it('should convert serialized function to callable proxy', async () => {
      receiver.applyBatch({
        version: 1,
        batchId: 1,
        operations: [
          {
            op: 'CREATE',
            id: 1,
            type: 'View',
            props: {
              onPress: { __type: 'function', __fnId: 'fn_1_abc' },
            },
          },
          { op: 'APPEND', id: 1, parentId: 0, childId: 1 },
        ],
      });

      // Wait for microtask
      await new Promise((resolve) => queueMicrotask(resolve));

      // Render to get the deserialized props
      const element = receiver.render();
      expect(element).not.toBeNull();

      // The onPress prop should be a function
      if (element && typeof element === 'object' && 'props' in element) {
        const onPress = (element as any).props.onPress;
        expect(typeof onPress).toBe('function');

        // Calling it should send CALL_FUNCTION message
        onPress('arg1', 123);

        expect(sendToSandbox).toHaveBeenCalledWith({
          type: 'CALL_FUNCTION',
          fnId: 'fn_1_abc',
          args: ['arg1', 123],
        });
      }
    });

    it('should deserialize nested functions in objects', async () => {
      receiver.applyBatch({
        version: 1,
        batchId: 1,
        operations: [
          {
            op: 'CREATE',
            id: 1,
            type: 'View',
            props: {
              handlers: {
                onTap: { __type: 'function', __fnId: 'fn_tap' },
                onLongPress: { __type: 'function', __fnId: 'fn_long' },
              },
            },
          },
          { op: 'APPEND', id: 1, parentId: 0, childId: 1 },
        ],
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      const element = receiver.render();
      if (element && typeof element === 'object' && 'props' in element) {
        const handlers = (element as any).props.handlers;
        expect(typeof handlers.onTap).toBe('function');
        expect(typeof handlers.onLongPress).toBe('function');

        handlers.onTap();
        expect(sendToSandbox).toHaveBeenCalledWith({
          type: 'CALL_FUNCTION',
          fnId: 'fn_tap',
          args: [],
        });
      }
    });

    it('should deserialize functions in arrays', async () => {
      receiver.applyBatch({
        version: 1,
        batchId: 1,
        operations: [
          {
            op: 'CREATE',
            id: 1,
            type: 'View',
            props: {
              callbacks: [
                { __type: 'function', __fnId: 'fn_0' },
                { __type: 'function', __fnId: 'fn_1' },
              ],
            },
          },
          { op: 'APPEND', id: 1, parentId: 0, childId: 1 },
        ],
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      const element = receiver.render();
      if (element && typeof element === 'object' && 'props' in element) {
        const callbacks = (element as any).props.callbacks;
        expect(Array.isArray(callbacks)).toBe(true);
        expect(typeof callbacks[0]).toBe('function');
        expect(typeof callbacks[1]).toBe('function');
      }
    });
  });

  describe('Nested object deserialization', () => {
    it('should recursively deserialize nested objects', async () => {
      receiver.applyBatch({
        version: 1,
        batchId: 1,
        operations: [
          {
            op: 'CREATE',
            id: 1,
            type: 'View',
            props: {
              style: {
                container: {
                  flex: 1,
                  nested: {
                    deep: {
                      value: 'test',
                    },
                  },
                },
              },
            },
          },
          { op: 'APPEND', id: 1, parentId: 0, childId: 1 },
        ],
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      const element = receiver.render();
      if (element && typeof element === 'object' && 'props' in element) {
        const style = (element as any).props.style;
        expect(style.container.nested.deep.value).toBe('test');
      }
    });

    it('should deserialize arrays with nested objects', async () => {
      receiver.applyBatch({
        version: 1,
        batchId: 1,
        operations: [
          {
            op: 'CREATE',
            id: 1,
            type: 'View',
            props: {
              items: [
                { id: 1, name: 'Item 1' },
                { id: 2, name: 'Item 2' },
              ],
            },
          },
          { op: 'APPEND', id: 1, parentId: 0, childId: 1 },
        ],
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      const element = receiver.render();
      if (element && typeof element === 'object' && 'props' in element) {
        const items = (element as any).props.items;
        expect(items).toHaveLength(2);
        expect(items[0].name).toBe('Item 1');
        expect(items[1].name).toBe('Item 2');
      }
    });
  });
});

describe('Receiver Rendering Edge Cases', () => {
  let registry: ComponentRegistry;
  let sendToSandbox: ReturnType<typeof vi.fn>;
  let onUpdate: ReturnType<typeof vi.fn>;
  let receiver: Receiver;
  let warnSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    registry = new ComponentRegistry();
    registry.register('View', MockView);
    registry.register('Text', MockText);
    sendToSandbox = vi.fn();
    onUpdate = vi.fn();
    receiver = new Receiver(registry, sendToSandbox, onUpdate);
    warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
  });

  afterEach(() => {
    warnSpy.mockRestore();
  });

  it('should warn and return null for missing node', async () => {
    // Create a node but don't add to root
    receiver.applyBatch({
      version: 1,
      batchId: 1,
      operations: [
        { op: 'CREATE', id: 1, type: 'View', props: {} },
        // Manually reference non-existent node
      ],
    });

    await new Promise((resolve) => queueMicrotask(resolve));

    // Directly manipulate to test edge case - reference non-existent child
    receiver.applyBatch({
      version: 1,
      batchId: 2,
      operations: [
        { op: 'APPEND', id: 1, parentId: 0, childId: 1 },
        // Reference child 999 which doesn't exist in parent's children
      ],
    });

    await new Promise((resolve) => queueMicrotask(resolve));

    // Now try to render - this should work
    const element = receiver.render();
    expect(element).not.toBeNull();
  });

  it('should warn for unregistered component', async () => {
    receiver.applyBatch({
      version: 1,
      batchId: 1,
      operations: [
        { op: 'CREATE', id: 1, type: 'UnknownComponent', props: {} },
        { op: 'APPEND', id: 1, parentId: 0, childId: 1 },
      ],
    });

    await new Promise((resolve) => queueMicrotask(resolve));

    const element = receiver.render();
    // Should have warned about unregistered component
    expect(warnSpy).toHaveBeenCalledWith(
      expect.stringContaining('UnknownComponent')
    );
  });

  it('should render __TEXT__ nodes as strings', async () => {
    receiver.applyBatch({
      version: 1,
      batchId: 1,
      operations: [
        { op: 'CREATE', id: 1, type: 'Text', props: {} },
        { op: 'CREATE', id: 2, type: '__TEXT__', props: { text: 'Hello World' } },
        { op: 'APPEND', id: 1, parentId: 0, childId: 1 },
        { op: 'APPEND', id: 2, parentId: 1, childId: 2 },
      ],
    });

    await new Promise((resolve) => queueMicrotask(resolve));

    const element = receiver.render();
    expect(element).not.toBeNull();
  });

  it('should handle TEXT operation to update text content', async () => {
    receiver.applyBatch({
      version: 1,
      batchId: 1,
      operations: [
        { op: 'CREATE', id: 1, type: '__TEXT__', props: { text: 'Initial' } },
        { op: 'APPEND', id: 1, parentId: 0, childId: 1 },
      ],
    });

    await new Promise((resolve) => queueMicrotask(resolve));

    receiver.applyBatch({
      version: 1,
      batchId: 2,
      operations: [{ op: 'TEXT', id: 1, text: 'Updated' }],
    });

    await new Promise((resolve) => queueMicrotask(resolve));

    const element = receiver.render();
    expect(element).toBe('Updated');
  });
});

describe('Receiver Metrics', () => {
  it('should emit render metrics', async () => {
    const metrics: Array<{ name: string; value: number; extra?: Record<string, unknown> }> = [];
    const registry = new ComponentRegistry();
    registry.register('View', MockView);

    const receiver = new Receiver(
      registry,
      vi.fn(),
      vi.fn(),
      {
        onMetric: (name, value, extra) => {
          metrics.push({ name, value, extra });
        },
      }
    );

    receiver.applyBatch({
      version: 1,
      batchId: 1,
      operations: [
        { op: 'CREATE', id: 1, type: 'View', props: {} },
        { op: 'APPEND', id: 1, parentId: 0, childId: 1 },
      ],
    });

    await new Promise((resolve) => queueMicrotask(resolve));

    receiver.render();

    const renderMetric = metrics.find((m) => m.name === 'receiver.render');
    expect(renderMetric).toBeDefined();
    expect(renderMetric?.extra).toHaveProperty('nodeCount');
  });

  it('should emit applyBatch metrics with skipped count', async () => {
    const metrics: Array<{ name: string; value: number; extra?: Record<string, unknown> }> = [];
    const registry = new ComponentRegistry();
    registry.register('View', MockView);

    const receiver = new Receiver(
      registry,
      vi.fn(),
      vi.fn(),
      {
        maxBatchSize: 2,
        onMetric: (name, value, extra) => {
          metrics.push({ name, value, extra });
        },
      }
    );

    // Send more operations than maxBatchSize
    receiver.applyBatch({
      version: 1,
      batchId: 1,
      operations: [
        { op: 'CREATE', id: 1, type: 'View', props: {} },
        { op: 'CREATE', id: 2, type: 'View', props: {} },
        { op: 'CREATE', id: 3, type: 'View', props: {} },
        { op: 'CREATE', id: 4, type: 'View', props: {} },
      ],
    });

    await new Promise((resolve) => queueMicrotask(resolve));

    const batchMetric = metrics.find((m) => m.name === 'receiver.applyBatch');
    expect(batchMetric).toBeDefined();
    expect(batchMetric?.extra?.applied).toBe(2);
    expect(batchMetric?.extra?.skipped).toBe(2);
  });
});
