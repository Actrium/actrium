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

  describe('handleGuestMessage - invalid reserved command payloads', () => {
    it('should handle invalid payload shape for ASKIT_TOAST_SHOW without crashing', () => {
      expect(() => handleGuestMessage({ event: 'ASKIT_TOAST_SHOW', payload: null })).not.toThrow();
      expect(() =>
        handleGuestMessage({ event: 'ASKIT_TOAST_SHOW', payload: { message: 123 } })
      ).not.toThrow();
    });

    it('should handle invalid payload shape for ASKIT_HAPTIC_TRIGGER without crashing', () => {
      expect(() =>
        handleGuestMessage({ event: 'ASKIT_HAPTIC_TRIGGER', payload: null })
      ).not.toThrow();
      expect(() =>
        handleGuestMessage({ event: 'ASKIT_HAPTIC_TRIGGER', payload: { type: 123 } })
      ).not.toThrow();
    });
  });

  describe('handleGuestMessage - contract validation errors', () => {
    it('should handle invalid payload types without crashing', () => {
      expect(() =>
        handleGuestMessage({
          event: 'ASKIT_TOAST_SHOW',
          payload: { message: 123 }, // Invalid type, but won't crash
        })
      ).not.toThrow();
    });
  });
});
