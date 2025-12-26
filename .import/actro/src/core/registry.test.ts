/**
 * Core Registry Tests
 *
 * Tests module registry and configuration functions
 * Note: Full registry tests require React Native environment
 *
 * We test the modules independently since registry.ts
 * imports React Native components which can't be loaded in Node.
 */

import { Haptic, type HostHapticInternal } from '../api/Haptic.host';
import { Toast, type HostToastInternal } from '../api/Toast.host';

// Recreate module registry (same as registry.ts)
const modules = {
  toast: Toast,
  haptic: Haptic,
} as const;

describe('Core Registry', () => {
  describe('Module Registry', () => {
    it('should export toast module', () => {
      expect(modules.toast).toBeDefined();
      expect(typeof modules.toast.show).toBe('function');
    });

    it('should export haptic module', () => {
      expect(modules.haptic).toBeDefined();
      expect(typeof modules.haptic.trigger).toBe('function');
    });
  });

  describe('Toast Module', () => {
    let toastCalls: Array<{ message: string; options: unknown }>;

    beforeEach(() => {
      toastCalls = [];
      (Toast as HostToastInternal)._setHandler((message, options) => {
        toastCalls.push({ message, options });
      });
    });

    afterEach(() => {
      (Toast as HostToastInternal)._clearHandler();
    });

    it('should call show method', () => {
      Toast.show('Hello', { position: 'top' });

      expect(toastCalls).toEqual([{ message: 'Hello', options: { position: 'top' } }]);
    });

    it('should call show without options', () => {
      Toast.show('Message');

      expect(toastCalls).toEqual([{ message: 'Message', options: undefined }]);
    });

    it('should call show with duration options', () => {
      Toast.show('Test', { duration: 'long' });

      expect(toastCalls[0]?.options).toEqual({ duration: 'long' });
    });
  });

  describe('Haptic Module', () => {
    let hapticCalls: Array<{ type: unknown }>;

    beforeEach(() => {
      hapticCalls = [];
      (Haptic as HostHapticInternal)._setHandler((type) => {
        hapticCalls.push({ type });
      });
    });

    afterEach(() => {
      (Haptic as HostHapticInternal)._clearHandler();
    });

    it('should call trigger method', () => {
      Haptic.trigger('light');

      expect(hapticCalls).toEqual([{ type: 'light' }]);
    });

    it('should handle all haptic types', () => {
      const types = ['light', 'medium', 'heavy', 'selection', 'success', 'warning', 'error'];
      for (const type of types) {
        Haptic.trigger(type as Parameters<typeof Haptic.trigger>[0]);
      }

      expect(hapticCalls.map((c) => c.type)).toEqual(types);
    });

    it('should use default type when not specified', () => {
      Haptic.trigger();

      expect(hapticCalls).toEqual([{ type: 'medium' }]);
    });
  });

  describe('configureToast function', () => {
    afterEach(() => {
      (Toast as HostToastInternal)._clearHandler();
    });

    it('should set custom toast handler', () => {
      const calls: string[] = [];

      (Toast as HostToastInternal)._setHandler((message) => {
        calls.push(message);
      });

      Toast.show('Test');

      expect(calls).toEqual(['Test']);
    });

    it('should pass options to custom handler', () => {
      const calls: Array<{ message: string; options: unknown }> = [];

      (Toast as HostToastInternal)._setHandler((message, options) => {
        calls.push({ message, options });
      });

      Toast.show('Msg', { duration: 'long' });

      expect(calls[0]?.options).toEqual({ duration: 'long' });
    });
  });

  describe('configureHaptic function', () => {
    afterEach(() => {
      (Haptic as HostHapticInternal)._clearHandler();
    });

    it('should set custom haptic handler', () => {
      const calls: unknown[] = [];

      (Haptic as HostHapticInternal)._setHandler((type) => {
        calls.push(type);
      });

      Haptic.trigger('success');

      expect(calls).toEqual(['success']);
    });
  });
});
