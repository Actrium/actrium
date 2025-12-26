/**
 * Core Bridge Tests
 *
 * These tests validate the real implementation in `src/core/bridge.ts`.
 */

import { describe, expect, it, afterEach } from 'bun:test';

import { EventEmitter, type HostEventEmitter } from '../api/EventEmitter.host';
import { createEngineAdapter, handleGuestMessage } from './bridge';
import {
  clearHapticHandler,
  clearToastHandler,
  configureHaptic,
  configureToast,
} from './registry.modules';

describe('Core Bridge', () => {
  afterEach(() => {
    clearToastHandler();
    clearHapticHandler();
  });

  describe('handleGuestMessage', () => {
    describe('askit reserved command routing', () => {
      it('should route ASKIT_TOAST_SHOW to Toast module', () => {
        const toastCalls: Array<{ message: string; options?: unknown }> = [];
        configureToast((message, options) => toastCalls.push({ message, options }));

        handleGuestMessage({
          event: 'ASKIT_TOAST_SHOW',
          payload: { message: 'Hello', options: { position: 'top' } },
        });

        expect(toastCalls).toEqual([{ message: 'Hello', options: { position: 'top' } }]);
      });

      it('should route ASKIT_HAPTIC_TRIGGER to Haptic module', () => {
        const hapticCalls: Array<{ type?: unknown }> = [];
        configureHaptic((type) => hapticCalls.push({ type }));

        handleGuestMessage({
          event: 'ASKIT_HAPTIC_TRIGGER',
          payload: { type: 'success' },
        });

        expect(hapticCalls).toEqual([{ type: 'success' }]);
      });

      it('should handle missing payload for ASKIT_HAPTIC_TRIGGER (uses default)', () => {
        const hapticCalls: Array<{ type?: unknown }> = [];
        configureHaptic((type) => hapticCalls.push({ type }));

        handleGuestMessage({ event: 'ASKIT_HAPTIC_TRIGGER' });

        // HostHaptic defaults to 'medium'
        expect(hapticCalls).toEqual([{ type: 'medium' }]);
      });
    });

    describe('app event routing', () => {
      it('should forward unknown/non-contract events to EventEmitter', () => {
        const received: unknown[] = [];
        const unsubscribe = (EventEmitter as HostEventEmitter).on('user:login', (payload) => {
          received.push(payload);
        }) as unknown as () => void;

        handleGuestMessage({ event: 'user:login', payload: { userId: 123 } });

        expect(received).toEqual([{ userId: 123 }]);
        unsubscribe();
      });

      it('should forward random events to EventEmitter', () => {
        const received: unknown[] = [];
        const unsubscribe = (EventEmitter as HostEventEmitter).on('randomstring', (payload) => {
          received.push(payload);
        }) as unknown as () => void;

        handleGuestMessage({ event: 'randomstring', payload: 'data' });

        expect(received).toEqual(['data']);
        unsubscribe();
      });
    });
  });

  describe('createEngineAdapter', () => {
    it('should forward Host EventEmitter events to engine.sendEvent', () => {
      const sentEvents: Array<{ event: string; payload: unknown }> = [];
      const engine = {
        on: () => () => {},
        sendEvent: (event: string, payload?: unknown) => {
          sentEvents.push({ event, payload });
        },
      };

      createEngineAdapter(engine);

      (EventEmitter as HostEventEmitter).emit('hostEventForward', { data: 'toGuest' });

      expect(
        sentEvents.some(
          (e) =>
            e.event === 'hostEventForward' &&
            JSON.stringify(e.payload) === JSON.stringify({ data: 'toGuest' })
        )
      ).toBe(true);
    });

    it('should route engine messages to handleGuestMessage', () => {
      const toastCalls: string[] = [];
      configureToast((message) => toastCalls.push(message));

      let messageCallback: ((message: { event: string; payload?: unknown }) => void) | null = null;

      // Use a loosely-typed engine mock here to avoid TypeScript contravariance
      // issues between the adapter's expected Engine type and this test double.
      const engine = {
        on: (event: string, callback: (message: { event: string; payload?: unknown }) => void) => {
          if (event === 'message') messageCallback = callback;
          return () => {};
        },
        sendEvent: () => {},
      };

      createEngineAdapter(engine);

      if (messageCallback) {
        (messageCallback as unknown as (message: { event: string; payload?: unknown }) => void)({
          event: 'ASKIT_TOAST_SHOW',
          payload: { message: 'Engine Toast Message' },
        });
      }

      expect(toastCalls).toEqual(['Engine Toast Message']);
    });

    it('should call unsubscribe function on dispose', () => {
      let unsubscribed = false;
      const engine = {
        on: () => () => {
          unsubscribed = true;
        },
        sendEvent: () => {},
      };

      const adapter = createEngineAdapter(engine);
      adapter.dispose();

      expect(unsubscribed).toBe(true);
    });
  });
});
