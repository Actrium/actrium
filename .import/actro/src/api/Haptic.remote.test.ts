/**
 * Haptic Remote Implementation Tests
 *
 * 100% real tests - simulating sandbox environment
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { Haptic } from './Haptic.remote';

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

  describe('without sendToHost', () => {
    it('should warn when sendToHost is not available', () => {
      (globalThis as Record<string, unknown>)['sendToHost'] = undefined;

      const warnings: unknown[] = [];
      const originalWarn = console.warn;
      console.warn = (...args) => warnings.push(args);

      Haptic.trigger('light');

      expect(warnings.length).toBe(1);
      expect((warnings[0] as unknown[])?.[0]).toContain('sendToHost not available');

      console.warn = originalWarn;
    });
  });
});
