/**
 * Haptic Guest Implementation Tests
 *
 * 100% real tests - simulating sandbox environment
 */

import { Haptic } from './Haptic.guest';

describe('Haptic (Remote)', () => {
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

  describe('trigger', () => {
    it('should send haptic trigger command to host', () => {
      Haptic.trigger('light');

      expect(sentMessages).toEqual([
        {
          event: 'ASKIT_HAPTIC_TRIGGER',
          payload: { type: 'light' },
        },
      ]);
    });

    it('should use default type "medium" when not specified', () => {
      Haptic.trigger();

      expect(sentMessages[0]?.payload).toEqual({ type: 'medium' });
    });

    it('should handle all haptic types', () => {
      Haptic.trigger('light');
      Haptic.trigger('medium');
      Haptic.trigger('heavy');
      Haptic.trigger('selection');
      Haptic.trigger('success');
      Haptic.trigger('warning');
      Haptic.trigger('error');

      expect(sentMessages.map((m) => (m.payload as { type: string }).type)).toEqual([
        'light',
        'medium',
        'heavy',
        'selection',
        'success',
        'warning',
        'error',
      ]);
    });
  });

  describe('without __sendEventToHost', () => {
    it('should warn when __sendEventToHost is not available', () => {
      sandboxGlobal.__sendEventToHost = undefined;

      const warnings: unknown[] = [];
      const originalWarn = console.warn;
      console.warn = (...args) => warnings.push(args);

      Haptic.trigger('light');

      expect(warnings.length).toBeGreaterThanOrEqual(1);
      expect(JSON.stringify(warnings)).toContain('__sendEventToHost not available');

      console.warn = originalWarn;
    });
  });

  describe('error handling', () => {
    it('should handle __sendEventToHost throwing exception', () => {
      sandboxGlobal.__sendEventToHost = () => {
        throw new Error('Bridge failure');
      };

      const errors: unknown[] = [];
      const originalError = console.error;
      console.error = (...args) => errors.push(args);

      Haptic.trigger('success');

      expect(errors.length).toBeGreaterThanOrEqual(1);
      expect(JSON.stringify(errors)).toContain('Failed to send message to host');

      console.error = originalError;
    });
  });
});
