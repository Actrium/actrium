/**
 * Toast Host Implementation Tests
 *
 * Tests custom handler logic and fallback behavior
 * Android platform tests are in Toast.android.test.ts
 */

import {
  HostToast,
  getDurationMs,
  getGravityValue,
  TOAST_SET_HANDLER as SET_HANDLER_SYMBOL,
  TOAST_CLEAR_HANDLER as CLEAR_HANDLER_SYMBOL,
} from './Toast.host';
import type { ToastOptions } from '../types';

describe('Toast (Host)', () => {
  describe('getDurationMs', () => {
    it('should return 2000 for "short"', () => {
      expect(getDurationMs('short')).toBe(2000);
    });

    it('should return 3500 for "long"', () => {
      expect(getDurationMs('long')).toBe(3500);
    });

    it('should return number directly when passed number', () => {
      expect(getDurationMs(5000)).toBe(5000);
      expect(getDurationMs(100)).toBe(100);
      expect(getDurationMs(0)).toBe(0);
    });

    it('should return 2000 for undefined', () => {
      expect(getDurationMs(undefined)).toBe(2000);
    });
  });

  describe('getGravityValue', () => {
    it('should return "top" for "top"', () => {
      expect(getGravityValue('top')).toBe('top');
    });

    it('should return "center" for "center"', () => {
      expect(getGravityValue('center')).toBe('center');
    });

    it('should return "bottom" for "bottom"', () => {
      expect(getGravityValue('bottom')).toBe('bottom');
    });

    it('should return "bottom" for undefined', () => {
      expect(getGravityValue(undefined)).toBe('bottom');
    });
  });

  describe('HostToast', () => {
    let toast: HostToast;

    beforeEach(() => {
      toast = new HostToast();
    });

    describe('with custom handler', () => {
      it('should use custom handler when set', () => {
        const calls: Array<{ message: string; options?: ToastOptions }> = [];

        toast[SET_HANDLER_SYMBOL]((message, options) => {
          calls.push({ message, options });
        });

        toast.show('Hello', { position: 'top' });

        expect(calls).toEqual([{ message: 'Hello', options: { position: 'top' } }]);
      });

      it('should pass options to custom handler', () => {
        const calls: Array<{ message: string; options?: ToastOptions }> = [];

        toast[SET_HANDLER_SYMBOL]((message, options) => {
          calls.push({ message, options });
        });

        toast.show('Test', { duration: 'long', position: 'center' });

        expect(calls[0]?.options).toEqual({ duration: 'long', position: 'center' });
      });

      it('should handle show without options', () => {
        const calls: Array<{ message: string; options?: ToastOptions }> = [];

        toast[SET_HANDLER_SYMBOL]((message, options) => {
          calls.push({ message, options });
        });

        toast.show('No options');

        expect(calls).toEqual([{ message: 'No options', options: undefined }]);
      });

      it('should allow clearing custom handler', () => {
        const calls: unknown[] = [];
        const logs: unknown[] = [];
        const originalLog = console.log;
        console.log = (...args) => logs.push(args);

        toast[SET_HANDLER_SYMBOL]((msg) => calls.push(msg));
        toast.show('First');

        toast[CLEAR_HANDLER_SYMBOL]();
        toast.show('Second');

        expect(calls).toEqual(['First']);
        // Second should go to fallback (console.log)
        expect(logs.length).toBe(1);
        expect((logs[0] as unknown[])?.[0]).toContain('Second');

        console.log = originalLog;
      });
    });

    describe('without custom handler (fallback)', () => {
      it('should log to console when no custom handler', () => {
        const logs: unknown[] = [];
        const originalLog = console.log;
        console.log = (...args) => logs.push(args);

        toast.show('Fallback message');

        expect(logs.length).toBe(1);
        expect((logs[0] as unknown[])?.[0]).toBe('[Toast] Fallback message');

        console.log = originalLog;
      });

      it('should handle multiple shows', () => {
        const logs: unknown[] = [];
        const originalLog = console.log;
        console.log = (...args) => logs.push(args);

        toast.show('First');
        toast.show('Second');
        toast.show('Third');

        expect(logs.length).toBe(3);
        expect((logs[0] as unknown[])?.[0]).toBe('[Toast] First');
        expect((logs[1] as unknown[])?.[0]).toBe('[Toast] Second');
        expect((logs[2] as unknown[])?.[0]).toBe('[Toast] Third');

        console.log = originalLog;
      });
    });

    describe('multiple instances', () => {
      it('should maintain separate handlers per instance', () => {
        const toast1 = new HostToast();
        const toast2 = new HostToast();
        const calls1: string[] = [];
        const calls2: string[] = [];

        toast1[SET_HANDLER_SYMBOL]((msg) => calls1.push(msg));
        toast2[SET_HANDLER_SYMBOL]((msg) => calls2.push(msg));

        toast1.show('msg1');
        toast2.show('msg2');

        expect(calls1).toEqual(['msg1']);
        expect(calls2).toEqual(['msg2']);
      });
    });
  });
});
