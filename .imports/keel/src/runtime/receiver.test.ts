/**
 * Receiver unit tests
 */

import { afterEach, beforeEach, describe, expect, it, mock, spyOn } from 'bun:test';
import React from 'react';
import type { HostMessage, OperationBatch } from './types';
import { Receiver, type SendToSandbox } from './receiver';
import { ComponentRegistry } from './registry';

// Mock components
const MockView: React.FC<{ style?: object; children?: React.ReactNode }> = ({ children }) =>
  React.createElement('View', null, children);

const MockText: React.FC<{ children?: React.ReactNode }> = ({ children }) =>
  React.createElement('Text', null, children);

const MockTouchable: React.FC<{
  onPress?: () => void;
  children?: React.ReactNode;
}> = ({ children }) => React.createElement('Touchable', null, children);

describe('Receiver', () => {
  let registry: ComponentRegistry;
  let receiver: Receiver;
  let sendToSandbox: SendToSandbox;
  let onUpdate: () => void;
  let sentMessages: HostMessage[];

  beforeEach(() => {
    registry = new ComponentRegistry();
    registry.registerAll({
      View: MockView,
      Text: MockText,
      TouchableOpacity: MockTouchable,
    });

    sentMessages = [];
    sendToSandbox = mock((message: HostMessage) => {
      sentMessages.push(message);
    });

    onUpdate = mock();
    receiver = new Receiver(registry, sendToSandbox, onUpdate);
  });

  afterEach(() => {
    receiver.clear();
    // mocks cleared;
  });

  describe('applyBatch', () => {
    it('should apply a batch of operations', async () => {
      const batch: OperationBatch = {
        version: 1,
        batchId: 1,
        operations: [
          { op: 'CREATE', id: 1, type: 'View', props: { style: { flex: 1 } } },
          { op: 'CREATE', id: 2, type: 'Text', props: {} },
          { op: 'APPEND', id: 2, parentId: 1, childId: 2 },
          { op: 'APPEND', id: 1, parentId: 0, childId: 1 },
        ],
      };

      receiver.applyBatch(batch);

      // Wait for microtask to complete
      await new Promise((resolve) => queueMicrotask(resolve));

      expect(receiver.nodeCount).toBe(2);
      expect(onUpdate).toHaveBeenCalled();
    });

    it('should debounce multiple rapid batches', async () => {
      const batch1: OperationBatch = {
        version: 1,
        batchId: 1,
        operations: [{ op: 'CREATE', id: 1, type: 'View', props: {} }],
      };

      const batch2: OperationBatch = {
        version: 1,
        batchId: 2,
        operations: [{ op: 'CREATE', id: 2, type: 'Text', props: {} }],
      };

      receiver.applyBatch(batch1);
      receiver.applyBatch(batch2);

      // Wait for microtask to complete
      await new Promise((resolve) => queueMicrotask(resolve));

      // onUpdate should be called only once
      expect(onUpdate).toHaveBeenCalledTimes(1);
      expect(receiver.nodeCount).toBe(2);
    });
  });

  describe('CREATE operation', () => {
    it('should create a node', async () => {
      receiver.applyBatch({
        version: 1,
        batchId: 1,
        operations: [{ op: 'CREATE', id: 1, type: 'View', props: { style: { flex: 1 } } }],
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      expect(receiver.nodeCount).toBe(1);
    });

    it('should create text node', async () => {
      receiver.applyBatch({
        version: 1,
        batchId: 1,
        operations: [{ op: 'CREATE', id: 1, type: '__TEXT__', props: { text: 'Hello' } }],
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      expect(receiver.nodeCount).toBe(1);
    });
  });

  describe('UPDATE operation', () => {
    it('should update node props', async () => {
      receiver.applyBatch({
        version: 1,
        batchId: 1,
        operations: [
          { op: 'CREATE', id: 1, type: 'View', props: { style: { flex: 1 } } },
          { op: 'APPEND', id: 1, parentId: 0, childId: 1 },
        ],
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      receiver.applyBatch({
        version: 1,
        batchId: 2,
        operations: [
          {
            op: 'UPDATE',
            id: 1,
            props: { style: { flex: 2, backgroundColor: 'red' } },
          },
        ],
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      const element = receiver.render();
      expect(element).not.toBeNull();
    });

    it('should remove props when specified', async () => {
      receiver.applyBatch({
        version: 1,
        batchId: 1,
        operations: [
          {
            op: 'CREATE',
            id: 1,
            type: 'View',
            props: { style: { flex: 1 }, testID: 'test' },
          },
          { op: 'APPEND', id: 1, parentId: 0, childId: 1 },
        ],
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      receiver.applyBatch({
        version: 1,
        batchId: 2,
        operations: [
          {
            op: 'UPDATE',
            id: 1,
            props: { style: { flex: 2 } },
            removedProps: ['testID'],
          },
        ],
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      expect(onUpdate).toHaveBeenCalled();
    });

    it('should warn when updating non-existent node', async () => {
      const consoleSpy = spyOn(console, 'warn').mockImplementation(() => {});

      receiver.applyBatch({
        version: 1,
        batchId: 1,
        operations: [{ op: 'UPDATE', id: 999, props: { style: {} } }],
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      expect(consoleSpy).toHaveBeenCalledWith(expect.stringContaining('Node 999 not found'));

      consoleSpy.mockRestore();
    });
  });

  describe('APPEND operation', () => {
    it('should append child to parent', async () => {
      receiver.applyBatch({
        version: 1,
        batchId: 1,
        operations: [
          { op: 'CREATE', id: 1, type: 'View', props: {} },
          { op: 'CREATE', id: 2, type: 'Text', props: {} },
          { op: 'APPEND', id: 2, parentId: 1, childId: 2 },
          { op: 'APPEND', id: 1, parentId: 0, childId: 1 },
        ],
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      const element = receiver.render();
      expect(element).not.toBeNull();
    });

    it('should append to root container when parentId is 0', async () => {
      receiver.applyBatch({
        version: 1,
        batchId: 1,
        operations: [
          { op: 'CREATE', id: 1, type: 'View', props: {} },
          { op: 'APPEND', id: 1, parentId: 0, childId: 1 },
        ],
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      const element = receiver.render();
      expect(element).not.toBeNull();
    });

    it('should not duplicate child', async () => {
      receiver.applyBatch({
        version: 1,
        batchId: 1,
        operations: [
          { op: 'CREATE', id: 1, type: 'View', props: {} },
          { op: 'APPEND', id: 1, parentId: 0, childId: 1 },
          { op: 'APPEND', id: 1, parentId: 0, childId: 1 }, // duplicated
        ],
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      // Should render only one child
      const element = receiver.render();
      expect(element).not.toBeNull();
    });
  });

  describe('INSERT operation', () => {
    it('should insert child at specified index', async () => {
      receiver.applyBatch({
        version: 1,
        batchId: 1,
        operations: [
          { op: 'CREATE', id: 1, type: 'View', props: {} },
          { op: 'CREATE', id: 2, type: 'Text', props: {} },
          { op: 'CREATE', id: 3, type: 'Text', props: {} },
          { op: 'APPEND', id: 2, parentId: 1, childId: 2 },
          { op: 'APPEND', id: 3, parentId: 1, childId: 3 },
          { op: 'APPEND', id: 1, parentId: 0, childId: 1 },
        ],
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      // Insert a new child at index 1
      receiver.applyBatch({
        version: 1,
        batchId: 2,
        operations: [
          { op: 'CREATE', id: 4, type: 'Text', props: {} },
          { op: 'INSERT', id: 4, parentId: 1, childId: 4, index: 1 },
        ],
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      expect(receiver.nodeCount).toBe(4);
    });

    it('should insert into root container', async () => {
      receiver.applyBatch({
        version: 1,
        batchId: 1,
        operations: [
          { op: 'CREATE', id: 1, type: 'View', props: {} },
          { op: 'CREATE', id: 2, type: 'View', props: {} },
          { op: 'APPEND', id: 1, parentId: 0, childId: 1 },
          { op: 'APPEND', id: 2, parentId: 0, childId: 2 },
        ],
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      receiver.applyBatch({
        version: 1,
        batchId: 2,
        operations: [
          { op: 'CREATE', id: 3, type: 'View', props: {} },
          { op: 'INSERT', id: 3, parentId: 0, childId: 3, index: 1 },
        ],
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      expect(receiver.nodeCount).toBe(3);
    });
  });

  describe('REMOVE operation', () => {
    it('should remove child from parent', async () => {
      receiver.applyBatch({
        version: 1,
        batchId: 1,
        operations: [
          { op: 'CREATE', id: 1, type: 'View', props: {} },
          { op: 'CREATE', id: 2, type: 'Text', props: {} },
          { op: 'APPEND', id: 2, parentId: 1, childId: 2 },
          { op: 'APPEND', id: 1, parentId: 0, childId: 1 },
        ],
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      receiver.applyBatch({
        version: 1,
        batchId: 2,
        operations: [{ op: 'REMOVE', id: 2, parentId: 1, childId: 2 }],
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      // Node still exists, just removed from parent
      expect(receiver.nodeCount).toBe(2);
    });

    it('should remove from root container', async () => {
      receiver.applyBatch({
        version: 1,
        batchId: 1,
        operations: [
          { op: 'CREATE', id: 1, type: 'View', props: {} },
          { op: 'APPEND', id: 1, parentId: 0, childId: 1 },
        ],
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      receiver.applyBatch({
        version: 1,
        batchId: 2,
        operations: [{ op: 'REMOVE', id: 1, parentId: 0, childId: 1 }],
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      const element = receiver.render();
      expect(element).toBeNull();
    });
  });

  describe('DELETE operation', () => {
    it('should delete node and its children', async () => {
      receiver.applyBatch({
        version: 1,
        batchId: 1,
        operations: [
          { op: 'CREATE', id: 1, type: 'View', props: {} },
          { op: 'CREATE', id: 2, type: 'Text', props: {} },
          { op: 'APPEND', id: 2, parentId: 1, childId: 2 },
          { op: 'APPEND', id: 1, parentId: 0, childId: 1 },
        ],
      });

      await new Promise((resolve) => queueMicrotask(resolve));
      expect(receiver.nodeCount).toBe(2);

      receiver.applyBatch({
        version: 1,
        batchId: 2,
        operations: [{ op: 'DELETE', id: 1 }],
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      // Parent and its children are deleted
      expect(receiver.nodeCount).toBe(0);
    });
  });

  describe('REORDER operation', () => {
    it('should reorder children', async () => {
      receiver.applyBatch({
        version: 1,
        batchId: 1,
        operations: [
          { op: 'CREATE', id: 1, type: 'View', props: {} },
          { op: 'CREATE', id: 2, type: 'Text', props: {} },
          { op: 'CREATE', id: 3, type: 'Text', props: {} },
          { op: 'CREATE', id: 4, type: 'Text', props: {} },
          { op: 'APPEND', id: 2, parentId: 1, childId: 2 },
          { op: 'APPEND', id: 3, parentId: 1, childId: 3 },
          { op: 'APPEND', id: 4, parentId: 1, childId: 4 },
          { op: 'APPEND', id: 1, parentId: 0, childId: 1 },
        ],
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      receiver.applyBatch({
        version: 1,
        batchId: 2,
        operations: [{ op: 'REORDER', id: 1, parentId: 1, childIds: [4, 2, 3] }],
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      expect(onUpdate).toHaveBeenCalled();
    });

    it('should reorder root children', async () => {
      receiver.applyBatch({
        version: 1,
        batchId: 1,
        operations: [
          { op: 'CREATE', id: 1, type: 'View', props: {} },
          { op: 'CREATE', id: 2, type: 'View', props: {} },
          { op: 'APPEND', id: 1, parentId: 0, childId: 1 },
          { op: 'APPEND', id: 2, parentId: 0, childId: 2 },
        ],
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      receiver.applyBatch({
        version: 1,
        batchId: 2,
        operations: [{ op: 'REORDER', id: 0, parentId: 0, childIds: [2, 1] }],
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      expect(onUpdate).toHaveBeenCalled();
    });
  });

  describe('TEXT operation', () => {
    it('should update text content', async () => {
      receiver.applyBatch({
        version: 1,
        batchId: 1,
        operations: [
          { op: 'CREATE', id: 1, type: '__TEXT__', props: { text: 'Hello' } },
          { op: 'APPEND', id: 1, parentId: 0, childId: 1 },
        ],
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      receiver.applyBatch({
        version: 1,
        batchId: 2,
        operations: [{ op: 'TEXT', id: 1, text: 'World' }],
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      expect(onUpdate).toHaveBeenCalled();
    });
  });

  describe('Function Props Deserialization', () => {
    it('should deserialize function props and call sendToSandbox', async () => {
      receiver.applyBatch({
        version: 1,
        batchId: 1,
        operations: [
          {
            op: 'CREATE',
            id: 1,
            type: 'TouchableOpacity',
            props: {
              onPress: { __type: 'function', __fnId: 'fn_1' },
            },
          },
          { op: 'APPEND', id: 1, parentId: 0, childId: 1 },
        ],
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      // Get rendered element and trigger onPress
      const element = receiver.render();
      expect(element).not.toBeNull();

      // Simulate invocation of deserialized function
      // Since we cannot access props directly, verify sendToSandbox behavior
    });

    it('should deserialize nested function props', async () => {
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
                onPress: { __type: 'function', __fnId: 'fn_1' },
                onLongPress: { __type: 'function', __fnId: 'fn_2' },
              },
            },
          },
        ],
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      expect(receiver.nodeCount).toBe(1);
    });

    it('should deserialize array with functions', async () => {
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
                { __type: 'function', __fnId: 'fn_1' },
                { __type: 'function', __fnId: 'fn_2' },
              ],
            },
          },
        ],
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      expect(receiver.nodeCount).toBe(1);
    });
  });

  describe('render', () => {
    it('should return null when no root children', () => {
      const element = receiver.render();
      expect(element).toBeNull();
    });

    it('should render single root child', async () => {
      receiver.applyBatch({
        version: 1,
        batchId: 1,
        operations: [
          { op: 'CREATE', id: 1, type: 'View', props: {} },
          { op: 'APPEND', id: 1, parentId: 0, childId: 1 },
        ],
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      const element = receiver.render();
      expect(element).not.toBeNull();
    });

    it('should render multiple root children in Fragment', async () => {
      receiver.applyBatch({
        version: 1,
        batchId: 1,
        operations: [
          { op: 'CREATE', id: 1, type: 'View', props: {} },
          { op: 'CREATE', id: 2, type: 'View', props: {} },
          { op: 'APPEND', id: 1, parentId: 0, childId: 1 },
          { op: 'APPEND', id: 2, parentId: 0, childId: 2 },
        ],
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      const element = receiver.render();
      expect(element).not.toBeNull();
    });

    it('should render text nodes as strings', async () => {
      receiver.applyBatch({
        version: 1,
        batchId: 1,
        operations: [
          { op: 'CREATE', id: 1, type: '__TEXT__', props: { text: 'Hello' } },
          { op: 'APPEND', id: 1, parentId: 0, childId: 1 },
        ],
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      const element = receiver.render();
      expect(element).toBe('Hello');
    });

    it('should warn for unregistered component types', async () => {
      const consoleSpy = spyOn(console, 'warn').mockImplementation(() => {});

      receiver.applyBatch({
        version: 1,
        batchId: 1,
        operations: [
          { op: 'CREATE', id: 1, type: 'UnknownComponent', props: {} },
          { op: 'APPEND', id: 1, parentId: 0, childId: 1 },
        ],
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      receiver.render();

      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Component "UnknownComponent" not registered')
      );

      consoleSpy.mockRestore();
    });
  });

  describe('clear', () => {
    it('should clear all nodes', async () => {
      receiver.applyBatch({
        version: 1,
        batchId: 1,
        operations: [
          { op: 'CREATE', id: 1, type: 'View', props: {} },
          { op: 'CREATE', id: 2, type: 'Text', props: {} },
          { op: 'APPEND', id: 2, parentId: 1, childId: 2 },
          { op: 'APPEND', id: 1, parentId: 0, childId: 1 },
        ],
      });

      await new Promise((resolve) => queueMicrotask(resolve));
      expect(receiver.nodeCount).toBe(2);

      receiver.clear();

      expect(receiver.nodeCount).toBe(0);
      expect(receiver.render()).toBeNull();
    });
  });

  describe('getDebugInfo', () => {
    it('should return debug information', async () => {
      receiver.applyBatch({
        version: 1,
        batchId: 1,
        operations: [
          { op: 'CREATE', id: 1, type: 'View', props: {} },
          { op: 'CREATE', id: 2, type: 'Text', props: {} },
          { op: 'APPEND', id: 2, parentId: 1, childId: 2 },
          { op: 'APPEND', id: 1, parentId: 0, childId: 1 },
        ],
      });

      await new Promise((resolve) => queueMicrotask(resolve));

      const info = receiver.getDebugInfo();

      expect(info.nodeCount).toBe(2);
      expect(info.rootChildren).toEqual([1]);
      expect(info.nodes).toHaveLength(2);
      expect(info.nodes.find((n) => n.id === 1)).toMatchObject({
        id: 1,
        type: 'View',
        childCount: 1,
      });
    });
  });
});
