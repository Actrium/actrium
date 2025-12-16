/**
 * Registry - Additional Coverage Tests
 * Tests for clearHapticHandler
 */

import { clearHapticHandler, configureHaptic } from './registry.modules';
import { Haptic } from '../api/Haptic.host';

describe('Registry - Additional Coverage', () => {
  describe('clearHapticHandler', () => {
    it('should clear custom haptic handler', () => {
      const customCalls: unknown[] = [];
      const customHandler = (type?: string) => customCalls.push(type);

      // Set custom handler
      configureHaptic(customHandler);
      Haptic.trigger('light');
      expect(customCalls).toEqual(['light']);

      // Clear handler
      clearHapticHandler();
      
      // After clearing, should use fallback (console.log)
      const originalLog = console.log;
      const logs: unknown[] = [];
      console.log = (...args: unknown[]) => logs.push(args);

      Haptic.trigger('medium');

      console.log = originalLog;

      // Should have logged (fallback behavior)
      expect(logs.length).toBeGreaterThan(0);
      // Custom handler should not have been called
      expect(customCalls).toEqual(['light']); // Still only the first call
    });
  });
});
