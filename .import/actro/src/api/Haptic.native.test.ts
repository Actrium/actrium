/**
 * Haptic Native Implementation Tests
 *
 * 100% real tests - no mocks
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { NativeHaptic } from './Haptic.native';
import type { HapticType } from '../types';

describe('Haptic (Native)', () => {
  let haptic: NativeHaptic;

  beforeEach(() => {
    haptic = new NativeHaptic();
  });

  describe('with custom handler', () => {
    it('should use custom handler when set', () => {
      const calls: HapticType[] = [];

      haptic._setCustomHandler((type) => {
        calls.push(type!);
      });

      haptic.trigger('light');

      expect(calls).toEqual(['light']);
    });

    it('should pass all haptic types correctly', () => {
      const calls: HapticType[] = [];

      haptic._setCustomHandler((type) => {
        calls.push(type!);
      });

      const types: HapticType[] = [
        'light',
        'medium',
        'heavy',
        'selection',
        'success',
        'warning',
        'error',
      ];
      types.forEach((type) => haptic.trigger(type));

      expect(calls).toEqual(types);
    });

    it('should use default type "medium" when not specified', () => {
      const calls: Array<HapticType | undefined> = [];

      haptic._setCustomHandler((type) => {
        calls.push(type);
      });

      haptic.trigger();

      expect(calls).toEqual(['medium']);
    });

    it('should allow clearing custom handler', () => {
      const calls: HapticType[] = [];
      const logs: unknown[] = [];
      const originalLog = console.log;
      console.log = (...args) => logs.push(args);

      haptic._setCustomHandler((type) => calls.push(type!));
      haptic.trigger('light');

      haptic._clearCustomHandler();
      haptic.trigger('heavy');

      expect(calls).toEqual(['light']);
      expect(logs.length).toBe(1);
      expect(logs[0][0]).toBe('[Haptic] heavy');

      console.log = originalLog;
    });
  });

  describe('without custom handler (fallback)', () => {
    it('should log to console when no custom handler', () => {
      const logs: unknown[] = [];
      const originalLog = console.log;
      console.log = (...args) => logs.push(args);

      haptic.trigger('success');

      expect(logs.length).toBe(1);
      expect(logs[0][0]).toBe('[Haptic] success');

      console.log = originalLog;
    });

    it('should log default type when not specified', () => {
      const logs: unknown[] = [];
      const originalLog = console.log;
      console.log = (...args) => logs.push(args);

      haptic.trigger();

      expect(logs[0][0]).toBe('[Haptic] medium');

      console.log = originalLog;
    });

    it('should handle multiple triggers', () => {
      const logs: unknown[] = [];
      const originalLog = console.log;
      console.log = (...args) => logs.push(args);

      haptic.trigger('light');
      haptic.trigger('heavy');
      haptic.trigger('error');

      expect(logs.length).toBe(3);
      expect(logs[0][0]).toBe('[Haptic] light');
      expect(logs[1][0]).toBe('[Haptic] heavy');
      expect(logs[2][0]).toBe('[Haptic] error');

      console.log = originalLog;
    });
  });

  describe('multiple instances', () => {
    it('should maintain separate handlers per instance', () => {
      const haptic1 = new NativeHaptic();
      const haptic2 = new NativeHaptic();
      const calls1: HapticType[] = [];
      const calls2: HapticType[] = [];

      haptic1._setCustomHandler((type) => calls1.push(type!));
      haptic2._setCustomHandler((type) => calls2.push(type!));

      haptic1.trigger('light');
      haptic2.trigger('heavy');

      expect(calls1).toEqual(['light']);
      expect(calls2).toEqual(['heavy']);
    });
  });
});
