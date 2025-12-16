/**
 * Bridge - Additional Coverage Tests
 * Tests for error handling branches
 */

import { handleGuestMessage } from './bridge';

describe('Core Bridge - Additional Coverage', () => {
  // Note: These tests verify that invalid inputs don't crash the bridge
  // We're not strictly checking console output to avoid test pollution issues

  describe('handleGuestMessage - invalid messages', () => {
    it('should handle null message without crashing', () => {
      // @ts-expect-error - Testing invalid input
      expect(() => handleGuestMessage(null)).not.toThrow();
    });

    it('should handle undefined message without crashing', () => {
      // @ts-expect-error - Testing invalid input
      expect(() => handleGuestMessage(undefined)).not.toThrow();
    });

    it('should handle message without event property without crashing', () => {
      // @ts-expect-error - Testing invalid input
      expect(() => handleGuestMessage({ payload: 'test' })).not.toThrow();
    });

    it('should handle message with non-string event without crashing', () => {
      // @ts-expect-error - Testing invalid input
      expect(() => handleGuestMessage({ event: 123, payload: 'test' })).not.toThrow();
    });
  });

  describe('handleGuestMessage - invalid askit protocol', () => {
    it('should handle invalid askit protocol format (too short) without crashing', () => {
      expect(() => handleGuestMessage({ event: 'askit:only', payload: null })).not.toThrow();
    });

    it('should handle unknown module without crashing', () => {
      expect(() =>
        handleGuestMessage({ event: 'askit:unknownmodule:method', payload: null })
      ).not.toThrow();
    });

    it('should handle unknown method without crashing', () => {
      expect(() =>
        handleGuestMessage({ event: 'askit:toast:unknownmethod', payload: null })
      ).not.toThrow();
    });
  });

  describe('handleGuestMessage - contract validation errors', () => {
    it('should handle invalid payload types without crashing', () => {
      // The bridge doesn't validate payload types strictly in production
      // It relies on TypeScript for compile-time safety
      // This test verifies the bridge doesn't crash with invalid data
      expect(() =>
        handleGuestMessage({
          event: 'askit:toast:show',
          payload: { message: 123 }, // Invalid type, but won't crash
        })
      ).not.toThrow();
    });
  });
});
