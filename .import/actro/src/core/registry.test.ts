/**
 * Core Registry Tests
 *
 * Tests module registry and configuration functions
 * Note: Full registry tests require React Native environment
 *
 * We test the modules independently since registry.ts
 * imports React Native components which can't be loaded in Node.
 */

import {
  HAPTIC_CLEAR_HANDLER,
  HAPTIC_SET_HANDLER,
  Haptic,
  type HostHaptic,
} from '../api/Haptic.host';
import {
  TOAST_CLEAR_HANDLER as CLEAR_HANDLER_SYMBOL,
  type HostToast,
  TOAST_SET_HANDLER as SET_HANDLER_SYMBOL,
  Toast,
} from '../api/Toast.host';

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
      (Toast as HostToast)[SET_HANDLER_SYMBOL]((message, options) => {
        toastCalls.push({ message, options });
      });
    });

    afterEach(() => {
      (Toast as HostToast)[CLEAR_HANDLER_SYMBOL]();
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
      (Haptic as HostHaptic)[HAPTIC_SET_HANDLER]((type) => {
        hapticCalls.push({ type });
      });
    });

    afterEach(() => {
      (Haptic as HostHaptic)[HAPTIC_CLEAR_HANDLER]();
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
      (Toast as HostToast)[CLEAR_HANDLER_SYMBOL]();
    });

    it('should set custom toast handler', () => {
      const calls: string[] = [];

      (Toast as HostToast)[SET_HANDLER_SYMBOL]((message) => {
        calls.push(message);
      });

      Toast.show('Test');

      expect(calls).toEqual(['Test']);
    });

    it('should pass options to custom handler', () => {
      const calls: Array<{ message: string; options: unknown }> = [];

      (Toast as HostToast)[SET_HANDLER_SYMBOL]((message, options) => {
        calls.push({ message, options });
      });

      Toast.show('Msg', { duration: 'long' });

      expect(calls[0]?.options).toEqual({ duration: 'long' });
    });
  });

  describe('configureHaptic function', () => {
    afterEach(() => {
      (Haptic as HostHaptic)[HAPTIC_CLEAR_HANDLER]();
    });

    it('should set custom haptic handler', () => {
      const calls: unknown[] = [];

      (Haptic as HostHaptic)[HAPTIC_SET_HANDLER]((type) => {
        calls.push(type);
      });

      Haptic.trigger('success');

      expect(calls).toEqual(['success']);
    });
  });
});
