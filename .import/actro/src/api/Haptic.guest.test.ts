/**
 * Haptic Guest Implementation Tests
 *
 * 100% real tests - simulating sandbox environment
 */

import { Haptic } from './Haptic.guest';

describe('Haptic (Remote)', () => {
  let sentMessages: Array<{ event: string; payload: unknown }>;
  let originalSendToHost: unknown;

  beforeEach(() => {
    originalSendToHost = (globalThis as Record<string, unknown>)['sendToHost'];
    sentMessages = [];

    (globalThis as Record<string, unknown>)['sendToHost'] = (event: string, payload?: unknown) => {
      sentMessages.push({ event, payload });
    };
  });

  afterEach(() => {
    (globalThis as Record<string, unknown>)['sendToHost'] = originalSendToHost;
  });

  describe('trigger', () => {
    it('should send haptic trigger command to host', () => {
      Haptic.trigger('light');

      expect(sentMessages).toEqual([
        {
          event: 'askit:haptic:trigger',
          payload: ['light'],
        },
      ]);
    });

    it('should use default type "medium" when not specified', () => {
      Haptic.trigger();

      expect(sentMessages[0]?.payload).toEqual(['medium']);
    });

    it('should handle all haptic types', () => {
      Haptic.trigger('light');
      Haptic.trigger('medium');
      Haptic.trigger('heavy');
      Haptic.trigger('selection');
      Haptic.trigger('success');
      Haptic.trigger('warning');
      Haptic.trigger('error');

      expect(sentMessages.map((m) => (m.payload as string[])[0])).toEqual([
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

  describe('without sendToHost', () => {
    it('should warn when sendToHost is not available', () => {
      (globalThis as Record<string, unknown>)['sendToHost'] = undefined;

      const warnings: unknown[] = [];
      const originalWarn = console.warn;
      console.warn = (...args) => warnings.push(args);

      Haptic.trigger('light');

      expect(warnings.length).toBeGreaterThanOrEqual(1);
      expect(JSON.stringify(warnings)).toContain('sendToHost not available');

      console.warn = originalWarn;
    });
  });

  describe('error handling', () => {
    it('should handle sendToHost throwing exception', () => {
      (globalThis as Record<string, unknown>)['sendToHost'] = () => {
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
