/**
 * Toast Guest Implementation Tests
 *
 * 100% real tests - simulating sandbox environment
 */

import { Toast } from './Toast.guest';

describe('Toast (Remote)', () => {
  type SandboxGlobal = typeof globalThis & {
    __sendEventToHost?: (eventName: string, payload?: unknown) => void;
  };

  const sandboxGlobal = globalThis as SandboxGlobal;

  let sentMessages: Array<{ event: string; payload: unknown }>;
  let originalSendToHost: unknown;

  beforeEach(() => {
    originalSendToHost = sandboxGlobal.__sendEventToHost;
    sentMessages = [];

    sandboxGlobal.__sendEventToHost = (eventName: string, payload?: unknown) => {
      sentMessages.push({ event: eventName, payload });
    };
  });

  afterEach(() => {
    sandboxGlobal.__sendEventToHost = originalSendToHost as SandboxGlobal['__sendEventToHost'];
  });

  describe('show', () => {
    it('should send toast show command to host', () => {
      Toast.show('Hello');

      expect(sentMessages).toEqual([
        {
          event: 'ASKIT_TOAST_SHOW',
          payload: { message: 'Hello', options: undefined },
        },
      ]);
    });

    it('should include options in payload', () => {
      Toast.show('Test', { position: 'top', duration: 'long' });

      expect(sentMessages[0]?.payload).toEqual({
        message: 'Test',
        options: { position: 'top', duration: 'long' },
      });
    });

    it('should handle numeric duration', () => {
      Toast.show('Numeric', { duration: 5000 });

      expect(sentMessages[0]?.payload).toEqual({ message: 'Numeric', options: { duration: 5000 } });
    });

    it('should handle position options', () => {
      Toast.show('Top', { position: 'top' });
      Toast.show('Center', { position: 'center' });
      Toast.show('Bottom', { position: 'bottom' });

      expect(sentMessages[0]?.payload).toEqual({ message: 'Top', options: { position: 'top' } });
      expect(sentMessages[1]?.payload).toEqual({
        message: 'Center',
        options: { position: 'center' },
      });
      expect(sentMessages[2]?.payload).toEqual({
        message: 'Bottom',
        options: { position: 'bottom' },
      });
    });

    it('should reject empty message', () => {
      const errors: unknown[] = [];
      const originalError = console.error;
      console.error = (...args) => errors.push(args);

      Toast.show('');

      expect(sentMessages.length).toBe(0);
      expect(errors.length).toBeGreaterThanOrEqual(1);
      expect(JSON.stringify(errors)).toContain('Invalid message parameter');

      console.error = originalError;
    });
  });

  describe('without __sendEventToHost', () => {
    it('should warn and log when __sendEventToHost is not available', () => {
      sandboxGlobal.__sendEventToHost = undefined;

      const warnings: unknown[] = [];
      const logs: unknown[] = [];
      const originalWarn = console.warn;
      const originalLog = console.log;
      console.warn = (...args) => warnings.push(args);
      console.log = (...args) => logs.push(args);

      Toast.show('Fallback');

      expect(JSON.stringify(warnings)).toContain('__sendEventToHost not available');
      expect(logs.length).toBeGreaterThanOrEqual(1);
      expect(JSON.stringify(logs)).toContain('[Toast] Fallback');

      console.warn = originalWarn;
      console.log = originalLog;
    });
  });

  describe('error handling', () => {
    it('should handle __sendEventToHost throwing exception', () => {
      sandboxGlobal.__sendEventToHost = () => {
        throw new Error('Bridge failure');
      };

      const errors: unknown[] = [];
      const logs: unknown[] = [];
      const originalError = console.error;
      const originalLog = console.log;
      console.error = (...args) => errors.push(args);
      console.log = (...args) => logs.push(args);

      Toast.show('Error test');

      expect(errors.length).toBeGreaterThanOrEqual(1);
      expect(JSON.stringify(errors)).toContain('Failed to send message to host');
      expect(logs.length).toBeGreaterThanOrEqual(1);
      expect(JSON.stringify(logs)).toContain('[Toast] Error test');

      console.error = originalError;
      console.log = originalLog;
    });

    it('should reject invalid message types', () => {
      const errors: unknown[] = [];
      const originalError = console.error;
      console.error = (...args) => errors.push(args);

      // @ts-expect-error - Testing invalid input
      Toast.show(null);
      // @ts-expect-error - Testing invalid input
      Toast.show(123);
      // @ts-expect-error - Testing invalid input
      Toast.show(undefined);

      expect(sentMessages.length).toBe(0);
      expect(errors.length).toBe(3);

      console.error = originalError;
    });
  });
});
