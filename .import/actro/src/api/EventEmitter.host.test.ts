/**
 * Bus Host Implementation Tests
 *
 * 100% real tests - no mocks, no fakes
 */

import { EventEmitter, HostEventEmitter, type HostEventEmitterInternal } from './EventEmitter.host';

describe('EventEmitter (Host)', () => {
  // Use a fresh instance for isolated tests
  let emitter: HostEventEmitter;

  beforeEach(() => {
    // Create fresh instance for each test (isolated from singleton)
    emitter = new HostEventEmitter();
  });

  describe('on/emit', () => {
    it('should receive emitted events', () => {
      const received: unknown[] = [];

      emitter.on('test', (payload) => {
        received.push(payload);
      });

      emitter.emit('test', { data: 'hello' });

      expect(received).toEqual([{ data: 'hello' }]);
    });

    it('should support multiple listeners for same event', () => {
      const received1: unknown[] = [];
      const received2: unknown[] = [];

      emitter.on('multi', (payload) => received1.push(payload));
      emitter.on('multi', (payload) => received2.push(payload));

      emitter.emit('multi', 'value');

      expect(received1).toEqual(['value']);
      expect(received2).toEqual(['value']);
    });

    it('should handle different event names independently', () => {
      const receivedA: unknown[] = [];
      const receivedB: unknown[] = [];

      emitter.on('eventA', (payload) => receivedA.push(payload));
      emitter.on('eventB', (payload) => receivedB.push(payload));

      emitter.emit('eventA', 'a');
      emitter.emit('eventB', 'b');

      expect(receivedA).toEqual(['a']);
      expect(receivedB).toEqual(['b']);
    });

    it('should handle emit without payload', () => {
      const received: unknown[] = [];

      emitter.on('noPayload', (payload) => {
        received.push(payload);
      });

      emitter.emit('noPayload');

      expect(received).toEqual([undefined]);
    });

    it('should handle emit with null payload', () => {
      const received: unknown[] = [];

      emitter.on('nullPayload', (payload) => {
        received.push(payload);
      });

      emitter.emit('nullPayload', null);

      expect(received).toEqual([null]);
    });

    it('should handle complex payloads', () => {
      const received: unknown[] = [];
      const complexPayload = {
        array: [1, 2, 3],
        nested: { deep: { value: true } },
        date: '2024-01-01',
      };

      emitter.on('complex', (payload) => {
        received.push(payload);
      });

      emitter.emit('complex', complexPayload);

      expect(received).toEqual([complexPayload]);
    });

    it('should not receive events when no listeners', () => {
      // Should not throw
      expect(() => emitter.emit('noListeners', 'data')).not.toThrow();
    });

    it('should catch errors in listeners without breaking other listeners', () => {
      const received: unknown[] = [];
      const consoleError = console.error;
      const errors: unknown[] = [];
      console.error = (...args) => errors.push(args);

      emitter.on('errorTest', () => {
        throw new Error('Listener error');
      });
      emitter.on('errorTest', (payload) => {
        received.push(payload);
      });

      emitter.emit('errorTest', 'value');

      // Second listener should still receive
      expect(received).toEqual(['value']);
      expect(errors.length).toBeGreaterThanOrEqual(1);

      console.error = consoleError;
    });
  });

  describe('off', () => {
    it('should unsubscribe listener', () => {
      const received: unknown[] = [];
      const callback = (payload: unknown) => received.push(payload);

      emitter.on('offTest', callback);
      emitter.emit('offTest', 'first');

      emitter.off('offTest', callback);
      emitter.emit('offTest', 'second');

      expect(received).toEqual(['first']);
    });

    it('should only remove specified callback', () => {
      const received1: unknown[] = [];
      const received2: unknown[] = [];
      const callback1 = (payload: unknown) => received1.push(payload);
      const callback2 = (payload: unknown) => received2.push(payload);

      emitter.on('partial', callback1);
      emitter.on('partial', callback2);

      emitter.off('partial', callback1);
      emitter.emit('partial', 'value');

      expect(received1).toEqual([]);
      expect(received2).toEqual(['value']);
    });

    it('should handle off for non-existent event', () => {
      const callback = () => {};
      expect(() => emitter.off('nonExistent', callback)).not.toThrow();
    });

    it('should handle off for non-existent callback', () => {
      emitter.on('exists', () => {});
      expect(() => emitter.off('exists', () => {})).not.toThrow();
    });

    it('should clean up empty listener sets', () => {
      const callback = () => {};
      emitter.on('cleanup', callback);
      emitter.off('cleanup', callback);

      // Internal check - listener map should be clean
      // Emit should not throw
      expect(() => emitter.emit('cleanup', 'data')).not.toThrow();
    });
  });

  describe('once', () => {
    it('should only fire once', () => {
      const received: unknown[] = [];

      emitter.once('onceTest', (payload) => {
        received.push(payload);
      });

      emitter.emit('onceTest', 'first');
      emitter.emit('onceTest', 'second');
      emitter.emit('onceTest', 'third');

      expect(received).toEqual(['first']);
    });

    it('should work with multiple once listeners', () => {
      const received1: unknown[] = [];
      const received2: unknown[] = [];

      emitter.once('multiOnce', (payload) => received1.push(payload));
      emitter.once('multiOnce', (payload) => received2.push(payload));

      emitter.emit('multiOnce', 'value');
      emitter.emit('multiOnce', 'value2');

      expect(received1).toEqual(['value']);
      expect(received2).toEqual(['value']);
    });

    it('should work together with on listeners', () => {
      const onceReceived: unknown[] = [];
      const onReceived: unknown[] = [];

      emitter.once('mixed', (payload) => onceReceived.push(payload));
      emitter.on('mixed', (payload) => onReceived.push(payload));

      emitter.emit('mixed', '1');
      emitter.emit('mixed', '2');

      expect(onceReceived).toEqual(['1']);
      expect(onReceived).toEqual(['1', '2']);
    });
  });

  describe('engine integration', () => {
    it('should set broadcaster and forward events', () => {
      const broadcasts: Array<{ event: string; payload: unknown }> = [];

      (emitter as HostEventEmitterInternal)._setBroadcaster((event, payload) => {
        broadcasts.push({ event, payload });
      });

      emitter.emit('engineEvent', 'data');

      expect(broadcasts).toEqual([{ event: 'engineEvent', payload: 'data' }]);

      (emitter as HostEventEmitterInternal)._setBroadcaster(null);
    });

    it('should unset broadcaster', () => {
      const broadcasts: Array<{ event: string; payload: unknown }> = [];

      (emitter as HostEventEmitterInternal)._setBroadcaster((event, payload) => {
        broadcasts.push({ event, payload });
      });

      emitter.emit('before', '1');
      (emitter as HostEventEmitterInternal)._setBroadcaster(null);
      emitter.emit('after', '2');

      expect(broadcasts).toEqual([{ event: 'before', payload: '1' }]);
    });

    it('should notify listeners directly', () => {
      const received: unknown[] = [];

      emitter.on('fromEngine', (payload) => {
        received.push(payload);
      });

      (emitter as HostEventEmitterInternal)._notifyLocal('fromEngine', { source: 'guest' });

      expect(received).toEqual([{ source: 'guest' }]);
    });

    it('should catch errors in _notifyLocal listeners', () => {
      const received: unknown[] = [];
      const consoleError = console.error;
      const errors: unknown[] = [];
      console.error = (...args) => errors.push(args);

      emitter.on('engineError', () => {
        throw new Error('Engine message handler error');
      });
      emitter.on('engineError', (payload) => {
        received.push(payload);
      });

      (emitter as HostEventEmitterInternal)._notifyLocal('engineError', 'data');

      // Second listener should still receive
      expect(received).toEqual(['data']);
      expect(errors.length).toBeGreaterThanOrEqual(1);

      console.error = consoleError;
    });

    it('should catch errors in engine broadcaster', () => {
      const consoleError = console.error;
      const errors: unknown[] = [];
      console.error = (...args) => errors.push(args);

      (emitter as HostEventEmitterInternal)._setBroadcaster(() => {
        throw new Error('Engine broadcast error');
      });

      expect(() => emitter.emit('errorEngine', 'data')).not.toThrow();
      expect(errors.length).toBeGreaterThanOrEqual(1);

      console.error = consoleError;
      (emitter as HostEventEmitterInternal)._setBroadcaster(null);
    });
  });

  describe('singleton export', () => {
    it('should export EventEmitter as EventEmitterAPI', () => {
      expect(EventEmitter).toBeDefined();
      expect(typeof EventEmitter.emit).toBe('function');
      expect(typeof EventEmitter.on).toBe('function');
      expect(typeof EventEmitter.off).toBe('function');
      expect(typeof EventEmitter.once).toBe('function');
    });
  });

  describe('wildcard pattern matching', () => {
    it('should match single-level wildcard (user:*)', () => {
      const received: unknown[] = [];

      emitter.on('user:*', (payload: unknown) => {
        received.push(payload);
      });

      emitter.emit('user:login', { id: 1 });
      emitter.emit('user:logout', { id: 2 });
      emitter.emit('admin:login', { id: 3 }); // Should NOT match

      expect(received).toEqual([{ id: 1 }, { id: 2 }]);
    });

    it('should match multi-level wildcard (analytics:**)', () => {
      const received: unknown[] = [];

      emitter.on('analytics:**', (payload: unknown) => {
        received.push(payload);
      });

      emitter.emit('analytics:click', 'button');
      emitter.emit('analytics:page:view', 'home');
      emitter.emit('analytics:user:profile:update', 'avatar');
      emitter.emit('metrics:cpu', 50); // Should NOT match

      expect(received).toEqual(['button', 'home', 'avatar']);
    });

    it('should support cleanup for pattern listeners', () => {
      const received: unknown[] = [];

      const unsubscribe = emitter.on('test:*', (payload: unknown) => {
        received.push(payload);
      });

      emitter.emit('test:one', 1);
      expect(received).toEqual([1]);

      unsubscribe();

      emitter.emit('test:two', 2);
      expect(received).toEqual([1]);
    });

    it('should trigger both exact and pattern listeners', () => {
      const exactReceived: unknown[] = [];
      const patternReceived: unknown[] = [];

      emitter.on('user:login', (payload: unknown) => {
        exactReceived.push(payload);
      });

      emitter.on('user:*', (payload: unknown) => {
        patternReceived.push(payload);
      });

      emitter.emit('user:login', 'data');

      expect(exactReceived).toEqual(['data']);
      expect(patternReceived).toEqual(['data']);
    });
  });
});
