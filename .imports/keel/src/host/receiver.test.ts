/**
 * Receiver Tests
 *
 * Tests for the operation receiver and remote ref functionality
 */

import { beforeEach, describe, expect, test } from 'bun:test';
import React from 'react';
import { CallbackRegistry } from '../guest/runtime/reconciler';
import { Receiver } from './receiver';
import { ComponentRegistry } from './registry';
import type { HostMessage, OperationBatch } from './types';

describe('Receiver - Remote Ref Support', () => {
  let receiver: Receiver;
  let registry: ComponentRegistry;
  let sentMessages: HostMessage[];
  let _updateCalled: boolean;
  let callbackRegistry: CallbackRegistry;

  beforeEach(() => {
    registry = new ComponentRegistry();
    sentMessages = [];
    _updateCalled = false;
    callbackRegistry = new CallbackRegistry();

    // Register mock components
    // biome-ignore lint/suspicious/noExplicitAny: Test/internal structure with dynamic types
    registry.register('View', 'View' as any);
    // biome-ignore lint/suspicious/noExplicitAny: Test/internal structure with dynamic types
    registry.register('TextInput', 'TextInput' as any);

    receiver = new Receiver(
      registry,
      (message) => {
        sentMessages.push(message);
      },
      () => {
        _updateCalled = true;
      },
      {
        callbackRegistry,
        debug: false,
      }
    );
  });

  describe('handleRefCall - Remote Ref Method Calls', () => {
    test('should handle successful ref method call', async () => {
      // Create a node with a ref
      const batch: OperationBatch = {
        version: 1,
        batchId: 1,
        operations: [
          {
            op: 'CREATE',
            id: 1,
            type: 'TextInput',
            props: { placeholder: 'Enter text' },
          },
        ],
      };

      receiver.applyBatch(batch);

      // Render to create refs
      receiver.render();

      // Mock the ref's current value with a focus method
      const nodes = receiver.getNodes();
      const node = nodes[0];
      expect(node).toBeDefined();

      // Manually set up ref with mock methods
      const mockRef = {
        focus: () => 'focused',
        blur: () => 'blurred',
      };

      // Access private refMap to inject mock ref
      // biome-ignore lint/suspicious/noExplicitAny: Test/internal structure with dynamic types
      const refMap = (receiver as any).refMap as Map<number, React.RefObject<unknown>>;
      const nodeRef = React.createRef<unknown>();
      // biome-ignore lint/suspicious/noExplicitAny: Test/internal structure with dynamic types
      (nodeRef as any).current = mockRef;
      refMap.set(1, nodeRef);

      // Call ref method
      const refCallBatch: OperationBatch = {
        version: 1,
        batchId: 2,
        operations: [
          {
            op: 'REF_CALL',
            id: 1,
            refId: 1,
            method: 'focus',
            args: [],
            callId: 'call_1',
          },
        ],
      };

      receiver.applyBatch(refCallBatch);

      // Wait for async ref call to complete
      await new Promise((resolve) => setTimeout(resolve, 50));

      // Should have sent result message
      expect(sentMessages.length).toBeGreaterThan(0);
      const resultMsg = sentMessages.find((msg) => msg.type === 'REF_METHOD_RESULT');
      expect(resultMsg).toBeDefined();
      // biome-ignore lint/suspicious/noExplicitAny: Test/internal structure with dynamic types
      expect((resultMsg as any).refId).toBe(1);
      // biome-ignore lint/suspicious/noExplicitAny: Test/internal structure with dynamic types
      expect((resultMsg as any).callId).toBe('call_1');
      // biome-ignore lint/suspicious/noExplicitAny: Test/internal structure with dynamic types
      expect((resultMsg as any).result).toBe('focused');
      // biome-ignore lint/suspicious/noExplicitAny: Test/internal structure with dynamic types
      expect((resultMsg as any).error).toBeUndefined();
    });

    test('should handle ref method call with arguments', async () => {
      const batch: OperationBatch = {
        version: 1,
        batchId: 1,
        operations: [
          {
            op: 'CREATE',
            id: 1,
            type: 'View',
            props: {},
          },
        ],
      };

      receiver.applyBatch(batch);
      receiver.render();

      // Mock ref with method that accepts arguments
      const mockRef = {
        scrollTo: (x: number, y: number) => ({ x, y }),
      };
      // biome-ignore lint/suspicious/noExplicitAny: Test/internal structure with dynamic types
      const refMap = (receiver as any).refMap as Map<number, React.RefObject<unknown>>;
      const nodeRef = React.createRef<unknown>();
      // biome-ignore lint/suspicious/noExplicitAny: Test/internal structure with dynamic types
      (nodeRef as any).current = mockRef;
      refMap.set(1, nodeRef);

      // Call method with arguments
      const refCallBatch: OperationBatch = {
        version: 1,
        batchId: 2,
        operations: [
          {
            op: 'REF_CALL',
            id: 1,
            refId: 1,
            method: 'scrollTo',
            args: [100, 200],
            callId: 'call_2',
          },
        ],
      };

      receiver.applyBatch(refCallBatch);

      await new Promise((resolve) => setTimeout(resolve, 50));

      const resultMsg = sentMessages.find((msg) => msg.type === 'REF_METHOD_RESULT');
      expect(resultMsg).toBeDefined();
      // biome-ignore lint/suspicious/noExplicitAny: Test/internal structure with dynamic types
      expect((resultMsg as any).result).toEqual({ x: 100, y: 200 });
    });

    test('should handle async ref method call', async () => {
      const batch: OperationBatch = {
        version: 1,
        batchId: 1,
        operations: [
          {
            op: 'CREATE',
            id: 1,
            type: 'View',
            props: {},
          },
        ],
      };

      receiver.applyBatch(batch);
      receiver.render();

      // Mock ref with async method
      const mockRef = {
        asyncMethod: async () => {
          await new Promise((resolve) => setTimeout(resolve, 10));
          return 'async result';
        },
      };
      // biome-ignore lint/suspicious/noExplicitAny: Test/internal structure with dynamic types
      const refMap = (receiver as any).refMap as Map<number, React.RefObject<unknown>>;
      const nodeRef = React.createRef<unknown>();
      // biome-ignore lint/suspicious/noExplicitAny: Test/internal structure with dynamic types
      (nodeRef as any).current = mockRef;
      refMap.set(1, nodeRef);

      const refCallBatch: OperationBatch = {
        version: 1,
        batchId: 2,
        operations: [
          {
            op: 'REF_CALL',
            id: 1,
            refId: 1,
            method: 'asyncMethod',
            args: [],
            callId: 'call_3',
          },
        ],
      };

      receiver.applyBatch(refCallBatch);

      await new Promise((resolve) => setTimeout(resolve, 100));

      const resultMsg = sentMessages.find((msg) => msg.type === 'REF_METHOD_RESULT');
      expect(resultMsg).toBeDefined();
      // biome-ignore lint/suspicious/noExplicitAny: Test/internal structure with dynamic types
      expect((resultMsg as any).result).toBe('async result');
    });

    test('should handle ref not found error', async () => {
      // Try to call method on non-existent ref
      const refCallBatch: OperationBatch = {
        version: 1,
        batchId: 1,
        operations: [
          {
            op: 'REF_CALL',
            id: 1,
            refId: 999, // Non-existent ref
            method: 'focus',
            args: [],
            callId: 'call_4',
          },
        ],
      };

      receiver.applyBatch(refCallBatch);

      await new Promise((resolve) => setTimeout(resolve, 50));

      const resultMsg = sentMessages.find((msg) => msg.type === 'REF_METHOD_RESULT');
      expect(resultMsg).toBeDefined();
      // biome-ignore lint/suspicious/noExplicitAny: Test/internal structure with dynamic types
      expect((resultMsg as any).error).toBeDefined();
      // biome-ignore lint/suspicious/noExplicitAny: Test/internal structure with dynamic types
      expect((resultMsg as any).error.__message).toContain('not mounted');
    });

    test('should handle method not found error', async () => {
      const batch: OperationBatch = {
        version: 1,
        batchId: 1,
        operations: [
          {
            op: 'CREATE',
            id: 1,
            type: 'View',
            props: {},
          },
        ],
      };

      receiver.applyBatch(batch);
      receiver.render();

      const mockRef = {
        focus: () => 'focused',
      };

      // biome-ignore lint/suspicious/noExplicitAny: Test/internal structure with dynamic types

      // biome-ignore lint/suspicious/noExplicitAny: Test/internal structure with dynamic types
      const refMap = (receiver as any).refMap as Map<number, React.RefObject<unknown>>;
      const nodeRef = React.createRef<unknown>();
      // biome-ignore lint/suspicious/noExplicitAny: Test/internal structure with dynamic types
      (nodeRef as any).current = mockRef;
      refMap.set(1, nodeRef);

      // Call non-existent method
      const refCallBatch: OperationBatch = {
        version: 1,
        batchId: 2,
        operations: [
          {
            op: 'REF_CALL',
            id: 1,
            refId: 1,
            method: 'nonExistentMethod',
            args: [],
            callId: 'call_5',
          },
        ],
      };

      receiver.applyBatch(refCallBatch);

      await new Promise((resolve) => setTimeout(resolve, 50));

      const resultMsg = sentMessages.find((msg) => msg.type === 'REF_METHOD_RESULT');
      expect(resultMsg).toBeDefined();
      // biome-ignore lint/suspicious/noExplicitAny: Test/internal structure with dynamic types
      expect((resultMsg as any).error).toBeDefined();
      // biome-ignore lint/suspicious/noExplicitAny: Test/internal structure with dynamic types
      expect((resultMsg as any).error.__message).toContain('not found');
    });

    test('should handle method execution error', async () => {
      const batch: OperationBatch = {
        version: 1,
        batchId: 1,
        operations: [
          {
            op: 'CREATE',
            id: 1,
            type: 'View',
            props: {},
          },
        ],
      };

      receiver.applyBatch(batch);
      receiver.render();

      // Mock ref with method that throws
      const mockRef = {
        throwingMethod: () => {
          throw new Error('Method execution failed');
        },
      };

      // biome-ignore lint/suspicious/noExplicitAny: Test/internal structure with dynamic types

      // biome-ignore lint/suspicious/noExplicitAny: Test/internal structure with dynamic types
      const refMap = (receiver as any).refMap as Map<number, React.RefObject<unknown>>;
      const nodeRef = React.createRef<unknown>();
      // biome-ignore lint/suspicious/noExplicitAny: Test/internal structure with dynamic types
      (nodeRef as any).current = mockRef;
      refMap.set(1, nodeRef);

      const refCallBatch: OperationBatch = {
        version: 1,
        batchId: 2,
        operations: [
          {
            op: 'REF_CALL',
            id: 1,
            refId: 1,
            method: 'throwingMethod',
            args: [],
            callId: 'call_6',
          },
        ],
      };

      receiver.applyBatch(refCallBatch);

      await new Promise((resolve) => setTimeout(resolve, 50));

      const resultMsg = sentMessages.find((msg) => msg.type === 'REF_METHOD_RESULT');
      expect(resultMsg).toBeDefined();
      // biome-ignore lint/suspicious/noExplicitAny: Test/internal structure with dynamic types
      expect((resultMsg as any).error).toBeDefined();
      // biome-ignore lint/suspicious/noExplicitAny: Test/internal structure with dynamic types
      expect((resultMsg as any).error.__message).toBe('Method execution failed');
    });

    test('should handle multiple concurrent ref calls', async () => {
      const batch: OperationBatch = {
        version: 1,
        batchId: 1,
        operations: [
          {
            op: 'CREATE',
            id: 1,
            type: 'View',
            props: {},
          },
          {
            op: 'CREATE',
            id: 2,
            type: 'View',
            props: {},
          },
        ],
      };

      receiver.applyBatch(batch);
      receiver.render();

      // Mock refs
      const mockRef1 = { method: () => 'result1' };
      const mockRef2 = { method: () => 'result2' };

      // biome-ignore lint/suspicious/noExplicitAny: Test/internal structure with dynamic types
      const refMap = (receiver as any).refMap as Map<number, React.RefObject<unknown>>;
      const nodeRef1 = React.createRef<unknown>();
      // biome-ignore lint/suspicious/noExplicitAny: Test/internal structure with dynamic types
      (nodeRef1 as any).current = mockRef1;
      refMap.set(1, nodeRef1);

      const nodeRef2 = React.createRef<unknown>();
      // biome-ignore lint/suspicious/noExplicitAny: Test/internal structure with dynamic types
      (nodeRef2 as any).current = mockRef2;
      refMap.set(2, nodeRef2);

      // Call multiple methods concurrently
      const refCallBatch: OperationBatch = {
        version: 1,
        batchId: 2,
        operations: [
          {
            op: 'REF_CALL',
            id: 1,
            refId: 1,
            method: 'method',
            args: [],
            callId: 'call_7',
          },
          {
            op: 'REF_CALL',
            id: 2,
            refId: 2,
            method: 'method',
            args: [],
            callId: 'call_8',
          },
        ],
      };

      receiver.applyBatch(refCallBatch);

      await new Promise((resolve) => setTimeout(resolve, 50));

      // Should have two result messages
      const resultMsgs = sentMessages.filter((msg) => msg.type === 'REF_METHOD_RESULT');
      expect(resultMsgs.length).toBe(2);
      // biome-ignore lint/suspicious/noExplicitAny: Test/internal structure with dynamic types
      const call7Result = resultMsgs.find((msg) => (msg as any).callId === 'call_7');
      // biome-ignore lint/suspicious/noExplicitAny: Test/internal structure with dynamic types
      const call8Result = resultMsgs.find((msg) => (msg as any).callId === 'call_8');

      expect(call7Result).toBeDefined();
      // biome-ignore lint/suspicious/noExplicitAny: Test/internal structure with dynamic types
      expect((call7Result as any).result).toBe('result1');
      expect(call8Result).toBeDefined();
      // biome-ignore lint/suspicious/noExplicitAny: Test/internal structure with dynamic types
      expect((call8Result as any).result).toBe('result2');
    });
  });
});
