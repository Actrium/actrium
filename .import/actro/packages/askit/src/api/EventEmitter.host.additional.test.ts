/**
 * EventEmitter Host - Additional Coverage Tests
 * Tests for setMaxListeners, getMaxListeners, and removeAllListeners
 */

import { HostEventEmitter } from './EventEmitter.host';

describe('EventEmitter (Host) - Additional Coverage', () => {
  let emitter: HostEventEmitter;

  beforeEach(() => {
    emitter = new HostEventEmitter();
  });

  describe('setMaxListeners and getMaxListeners', () => {
    it('should get default max listeners', () => {
      expect(emitter.getMaxListeners()).toBe(10);
    });

    it('should set and get max listeners', () => {
      emitter.setMaxListeners(20);
      expect(emitter.getMaxListeners()).toBe(20);
    });

    it('should clamp negative values to 0', () => {
      emitter.setMaxListeners(-5);
      expect(emitter.getMaxListeners()).toBe(0);
    });

    it('should clear warned events when setting max listeners', () => {
      // Set low limit and trigger warning
      emitter.setMaxListeners(1);
      emitter.on('test', () => {});
      emitter.on('test', () => {}); // Should warn

      // Reset max listeners should clear warnings
      emitter.setMaxListeners(10);
      expect(emitter.getMaxListeners()).toBe(10);
    });
  });

  describe('removeAllListeners', () => {
    it('should remove all listeners when called without arguments', () => {
      const calls: unknown[] = [];

      emitter.on('event1', (data) => calls.push(data));
      emitter.on('event2', (data) => calls.push(data));
      emitter.on('user:*', (data) => calls.push(data));

      // Remove all
      emitter.removeAllListeners();

      // Emit events - should not trigger any callbacks
      emitter.emit('event1', 'test1');
      emitter.emit('event2', 'test2');
      emitter.emit('user:login', 'test3');

      expect(calls.length).toBe(0);
    });

    it('should remove only exact match listeners for specific event', () => {
      const calls: unknown[] = [];

      emitter.on('remove:me', (data) => calls.push(['removed', data]));
      emitter.on('keep:me', (data) => calls.push(['kept', data]));

      // Remove specific event
      emitter.removeAllListeners('remove:me');

      emitter.emit('remove:me', 'data1');
      emitter.emit('keep:me', 'data2');

      expect(calls).toEqual([['kept', 'data2']]);
    });

    it('should remove pattern listeners for specific pattern', () => {
      const calls: unknown[] = [];

      emitter.on('user:*', (data) => calls.push(['pattern', data]));
      emitter.on('api:*', (data) => calls.push(['api', data]));

      // Remove pattern listeners
      emitter.removeAllListeners('user:*');

      emitter.emit('user:login', 'data1');
      emitter.emit('api:call', 'data2');

      expect(calls).toEqual([['api', 'data2']]);
    });
  });
});
