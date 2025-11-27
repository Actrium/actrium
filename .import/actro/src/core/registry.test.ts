/**
 * Core Registry Tests
 *
 * Tests module handlers and configuration functions
 * Note: Full registry tests require React Native environment
 *
 * We test the module handlers independently since registry.ts
 * imports React Native components which can't be loaded in Node.
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { Toast as ToastModule, NativeToast } from '../api/Toast.native';
import { Haptic as HapticModule, NativeHaptic } from '../api/Haptic.native';

// Recreate module handlers for testing (same logic as registry.ts)
interface ModuleHandler {
  handle: (method: string, args: unknown[]) => unknown;
}

const ToastHandler: ModuleHandler = {
  handle(method: string, args: unknown[]) {
    if (method === 'show') {
      const [message, options] = args as [string, unknown];
      ToastModule.show(message, options as Parameters<typeof ToastModule.show>[1]);
    }
  },
};

const HapticHandler: ModuleHandler = {
  handle(method: string, args: unknown[]) {
    if (method === 'trigger') {
      const [type] = args as [Parameters<typeof HapticModule.trigger>[0]];
      HapticModule.trigger(type);
    }
  },
};

describe('Core Registry (Module Handlers)', () => {
  describe('Toast Module Handler', () => {
    let toastCalls: Array<{ message: string; options: unknown }>;

    beforeEach(() => {
      toastCalls = [];
      (ToastModule as NativeToast)._setCustomHandler((message, options) => {
        toastCalls.push({ message, options });
      });
    });

    afterEach(() => {
      (ToastModule as NativeToast)._clearCustomHandler();
    });

    it('should handle show method', () => {
      ToastHandler.handle('show', ['Hello', { position: 'top' }]);

      expect(toastCalls).toEqual([{ message: 'Hello', options: { position: 'top' } }]);
    });

    it('should handle show without options', () => {
      ToastHandler.handle('show', ['Message']);

      expect(toastCalls).toEqual([{ message: 'Message', options: undefined }]);
    });

    it('should handle show with duration options', () => {
      ToastHandler.handle('show', ['Test', { duration: 'long' }]);

      expect(toastCalls[0].options).toEqual({ duration: 'long' });
    });

    it('should ignore unknown methods', () => {
      ToastHandler.handle('unknownMethod', ['arg']);

      expect(toastCalls).toEqual([]);
    });

    it('should handle empty args', () => {
      ToastHandler.handle('show', []);

      expect(toastCalls).toEqual([{ message: undefined, options: undefined }]);
    });
  });

  describe('Haptic Module Handler', () => {
    let hapticCalls: Array<{ type: unknown }>;

    beforeEach(() => {
      hapticCalls = [];
      (HapticModule as NativeHaptic)._setCustomHandler((type) => {
        hapticCalls.push({ type });
      });
    });

    afterEach(() => {
      (HapticModule as NativeHaptic)._clearCustomHandler();
    });

    it('should handle trigger method', () => {
      HapticHandler.handle('trigger', ['light']);

      expect(hapticCalls).toEqual([{ type: 'light' }]);
    });

    it('should handle trigger with all types', () => {
      const types = ['light', 'medium', 'heavy', 'selection', 'success', 'warning', 'error'];
      types.forEach((type) => HapticHandler.handle('trigger', [type]));

      expect(hapticCalls.map((c) => c.type)).toEqual(types);
    });

    it('should handle trigger without type (uses default)', () => {
      HapticHandler.handle('trigger', []);

      // Haptic.trigger(undefined) uses default 'medium'
      expect(hapticCalls).toEqual([{ type: 'medium' }]);
    });

    it('should ignore unknown methods', () => {
      HapticHandler.handle('unknownMethod', ['arg']);

      expect(hapticCalls).toEqual([]);
    });
  });

  describe('configureToast function', () => {
    afterEach(() => {
      (ToastModule as NativeToast)._clearCustomHandler();
    });

    it('should set custom toast handler', () => {
      const calls: string[] = [];

      (ToastModule as NativeToast)._setCustomHandler((message) => {
        calls.push(message);
      });

      ToastModule.show('Test');

      expect(calls).toEqual(['Test']);
    });

    it('should pass options to custom handler', () => {
      const calls: Array<{ message: string; options: unknown }> = [];

      (ToastModule as NativeToast)._setCustomHandler((message, options) => {
        calls.push({ message, options });
      });

      ToastModule.show('Msg', { duration: 'long' });

      expect(calls[0].options).toEqual({ duration: 'long' });
    });
  });

  describe('configureHaptic function', () => {
    afterEach(() => {
      (HapticModule as NativeHaptic)._clearCustomHandler();
    });

    it('should set custom haptic handler', () => {
      const calls: unknown[] = [];

      (HapticModule as NativeHaptic)._setCustomHandler((type) => {
        calls.push(type);
      });

      HapticModule.trigger('success');

      expect(calls).toEqual(['success']);
    });
  });
});
