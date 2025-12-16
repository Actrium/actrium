/**
 * Bridge Engine Adapter - Integration Tests
 * Tests for the actual createEngineAdapter implementation in bridge.ts
 */

import { createEngineAdapter, handleGuestMessage, type EngineInterface, type GuestMessageHandlerOptions } from './bridge';
import { EventEmitter, type HostEventEmitter } from '../api/EventEmitter.host';
import { Toast, type HostToast, TOAST_SET_HANDLER, TOAST_CLEAR_HANDLER } from '../api/Toast.host';

describe('Core Bridge - Engine Adapter (Real Implementation)', () => {
  describe('createEngineAdapter', () => {
    let toastCalls: Array<{ message: string; options?: unknown }>;
    let originalWarn: typeof console.warn;
    let originalError: typeof console.error;

    beforeEach(() => {
      toastCalls = [];
      originalWarn = console.warn;
      originalError = console.error;
      
      // Set up toast handler to capture calls
      (Toast as HostToast)[TOAST_SET_HANDLER]((message, options) => {
        toastCalls.push({ message, options });
      });
    });

    afterEach(() => {
      (Toast as HostToast)[TOAST_CLEAR_HANDLER]();
      console.warn = originalWarn;
      console.error = originalError;
    });

    it('should forward EventEmitter events to engine.sendEvent with askit:event: prefix', () => {
      const sentEvents: Array<{ event: string; payload: unknown }> = [];
      
      const mockEngine: EngineInterface = {
        on: () => () => {}, // Return unsubscribe function
        sendEvent: (event: string, payload?: unknown) => {
          sentEvents.push({ event, payload });
        },
      };

      createEngineAdapter(mockEngine);

      // Emit event from host EventEmitter
      (EventEmitter as HostEventEmitter).emit('test:event', { data: 'hello' });

      // Should be forwarded to engine with askit:event: prefix
      expect(sentEvents).toContainEqual({
        event: 'askit:event:test:event',
        payload: { data: 'hello' },
      });
    });

    it('should handle nested event names with multiple colons', () => {
      const sentEvents: Array<{ event: string; payload: unknown }> = [];
      
      const mockEngine: EngineInterface = {
        on: () => () => {},
        sendEvent: (event: string, payload?: unknown) => {
          sentEvents.push({ event, payload });
        },
      };

      createEngineAdapter(mockEngine);

      // Emit nested event
      (EventEmitter as HostEventEmitter).emit('analytics:user:login:success', { userId: 123 });

      expect(sentEvents).toContainEqual({
        event: 'askit:event:analytics:user:login:success',
        payload: { userId: 123 },
      });
    });

    it('should route incoming engine messages to handleGuestMessage', () => {
      let messageHandler: ((message: { event: string; payload?: unknown }) => void) | null = null;
      
      const mockEngine: EngineInterface = {
        on: (event: string, callback: (message: { event: string; payload?: unknown }) => void) => {
          if (event === 'message') {
            messageHandler = callback;
          }
          return () => {}; // Return unsubscribe function
        },
        sendEvent: () => {},
      };

      createEngineAdapter(mockEngine);

      // Simulate engine sending a message
      expect(messageHandler).not.toBeNull();
      messageHandler!({ event: 'askit:toast:show', payload: ['Hello from engine'] });

      // Should route to Toast
      expect(toastCalls).toEqual([{ message: 'Hello from engine', options: undefined }]);
    });

    it('should pass options to handleGuestMessage', () => {
      const violations: unknown[] = [];
      let messageHandler: ((message: { event: string; payload?: unknown }) => void) | null = null;
      
      const mockEngine: EngineInterface = {
        on: (event: string, callback: (message: { event: string; payload?: unknown }) => void) => {
          if (event === 'message') {
            messageHandler = callback;
          }
          return () => {};
        },
        sendEvent: () => {},
      };

      const options: GuestMessageHandlerOptions = {
        onContractViolation: (violation) => violations.push(violation),
      };

      createEngineAdapter(mockEngine, options);

      // Send unknown event
      messageHandler!({ event: 'unknown:event:type', payload: null });

      // Should call onContractViolation for unknown events
      expect(violations.length).toBeGreaterThan(0);
      const violation = violations[0] as any;
      expect(violation.kind).toBe('unknown_event');
      expect(violation.eventName).toBe('unknown:event:type');
    });

    it('should call unsubscribe function on dispose', () => {
      let unsubscribeCalled = false;
      const sentEvents: string[] = [];
      
      const mockEngine: EngineInterface = {
        on: () => {
          // Return unsubscribe function
          return () => {
            unsubscribeCalled = true;
          };
        },
        sendEvent: (event: string) => {
          sentEvents.push(event);
        },
      };

      const adapter = createEngineAdapter(mockEngine);

      // Emit event before dispose
      (EventEmitter as HostEventEmitter).emit('before:dispose', 'data1');
      expect(sentEvents).toContain('askit:event:before:dispose');

      // Dispose adapter
      adapter.dispose();
      expect(unsubscribeCalled).toBe(true);

      // Clear array
      sentEvents.length = 0;

      // Emit event after dispose - should not be forwarded
      (EventEmitter as HostEventEmitter).emit('after:dispose', 'data2');
      expect(sentEvents).not.toContain('askit:event:after:dispose');
    });

    it('should handle engine message with complex payload', () => {
      let messageHandler: ((message: { event: string; payload?: unknown }) => void) | null = null;
      
      const mockEngine: EngineInterface = {
        on: (event: string, callback: (message: { event: string; payload?: unknown }) => void) => {
          if (event === 'message') {
            messageHandler = callback;
          }
          return () => {};
        },
        sendEvent: () => {},
      };

      createEngineAdapter(mockEngine);

      // Send toast with complex options
      messageHandler!({ 
        event: 'askit:toast:show', 
        payload: ['Complex message', { duration: 'long', position: 'top' }] 
      });

      expect(toastCalls).toEqual([
        { message: 'Complex message', options: { duration: 'long', position: 'top' } }
      ]);
    });
  });

  describe('handleGuestMessage - EventEmitter NOTIFY_SYMBOL coverage', () => {
    it('should call NOTIFY_SYMBOL when receiving askit:event: messages', () => {
      const receivedEvents: Array<{ event: string; payload: unknown }> = [];
      
      // Listen to an event
      const unsubscribe = (EventEmitter as HostEventEmitter).on('coverage:test', (payload) => {
        receivedEvents.push({ event: 'coverage:test', payload });
      });

      // Send message through handleGuestMessage
      handleGuestMessage({ event: 'askit:event:coverage:test', payload: { test: 'data' } });

      // Should trigger the listener via NOTIFY_SYMBOL
      expect(receivedEvents).toEqual([
        { event: 'coverage:test', payload: { test: 'data' } }
      ]);

      unsubscribe();
    });

    it('should handle askit:event: with null payload', () => {
      const receivedEvents: unknown[] = [];
      
      const unsubscribe = (EventEmitter as HostEventEmitter).on('null:test', (payload) => {
        receivedEvents.push(payload);
      });

      handleGuestMessage({ event: 'askit:event:null:test', payload: null });

      expect(receivedEvents).toEqual([null]);

      unsubscribe();
    });

    it('should handle askit:event: with undefined payload', () => {
      const receivedEvents: unknown[] = [];
      
      const unsubscribe = (EventEmitter as HostEventEmitter).on('undefined:test', (payload) => {
        receivedEvents.push(payload);
      });

      handleGuestMessage({ event: 'askit:event:undefined:test' });

      expect(receivedEvents).toEqual([undefined]);

      unsubscribe();
    });
  });

  describe('handleGuestMessage - options parameter', () => {
    let toastCalls: Array<{ message: string; options?: unknown }>;
    let originalWarn: typeof console.warn;
    let originalError: typeof console.error;

    beforeEach(() => {
      toastCalls = [];
      originalWarn = console.warn;
      originalError = console.error;
      
      // Set up toast handler to capture calls
      (Toast as HostToast)[TOAST_SET_HANDLER]((message, options) => {
        toastCalls.push({ message, options });
      });
    });

    afterEach(() => {
      (Toast as HostToast)[TOAST_CLEAR_HANDLER]();
      console.warn = originalWarn;
      console.error = originalError;
    });

    it('should call onContractEvent when provided with valid contract event', () => {
      const contractEvents: Array<{ eventName: string; payload: unknown }> = [];
      
      // GUEST_SLEEP_STATE is a valid contract event
      handleGuestMessage(
        { event: 'GUEST_SLEEP_STATE', payload: { sleeping: true, tabId: 't1' } },
        {
          onContractEvent: (eventName, payload) => {
            contractEvents.push({ eventName, payload });
          },
        }
      );

      expect(contractEvents).toEqual([
        { eventName: 'GUEST_SLEEP_STATE', payload: { sleeping: true, tabId: 't1' } }
      ]);
    });

    it('should call onContractViolation when contract validation fails', () => {
      const violations: unknown[] = [];
      
      // Invalid payload for GUEST_SLEEP_STATE (missing required fields)
      handleGuestMessage(
        { event: 'GUEST_SLEEP_STATE', payload: { invalid: 'data' } },
        {
          onContractViolation: (violation) => {
            violations.push(violation);
          },
        }
      );

      expect(violations.length).toBeGreaterThan(0);
      const violation = violations[0] as any;
      expect(violation.kind).toBe('invalid_payload');
    });

    it('should call onContractViolation for unknown event formats', () => {
      const violations: unknown[] = [];
      
      handleGuestMessage(
        { event: 'some:random:event', payload: null },
        {
          onContractViolation: (violation) => {
            violations.push(violation);
          },
        }
      );

      expect(violations.length).toBeGreaterThan(0);
      const violation = violations[0] as any;
      expect(violation.kind).toBe('unknown_event');
      expect(violation.eventName).toBe('some:random:event');
    });

    it('should handle permission checks when permissionMode is deny', () => {
      const violations: unknown[] = [];
      
      handleGuestMessage(
        { event: 'askit:toast:show', payload: ['Test'] },
        {
          permissions: [], // No permissions declared
          permissionMode: 'deny',
          onContractViolation: (violation) => {
            violations.push(violation);
          },
        }
      );

      // Should violate due to missing permission
      expect(violations.length).toBeGreaterThan(0);
      const violation = violations[0] as any;
      expect(violation.kind).toBe('missing_permission');
    });

    it('should allow execution when permission is declared', () => {
      const callsBefore = toastCalls.length;
      
      handleGuestMessage(
        { event: 'askit:toast:show', payload: ['Test with permission'] },
        {
          permissions: ['toast'], // Permission declared
          permissionMode: 'deny',
        }
      );

      // Should execute without violation (toast handler should be called)
      expect(toastCalls.length).toBeGreaterThan(callsBefore);
      expect(toastCalls[toastCalls.length - 1]?.message).toBe('Test with permission');
    });
  });
});
