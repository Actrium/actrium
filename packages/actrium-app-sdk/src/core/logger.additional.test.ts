/**
 * Logger - Additional Coverage Tests
 * Tests for debug() method
 */

import { logger } from './logger';

describe('AskitLogger - Additional Coverage', () => {
  let originalDebug: typeof console.debug;

  beforeEach(() => {
    originalDebug = console.debug;
    logger.setLevel('info'); // Reset to default
  });

  afterEach(() => {
    console.debug = originalDebug;
    // Ensure logger level does not leak between tests/suites
    logger.setLevel('info');
  });

  describe('debug', () => {
    it('should log debug messages when level is debug', () => {
      const logs: unknown[] = [];
      console.debug = (...args: unknown[]) => logs.push(args);

      logger.setLevel('debug');
      logger.debug('TestModule', 'Debug message');

      expect(logs.length).toBeGreaterThan(0);
      expect(JSON.stringify(logs)).toContain('TestModule');
      expect(JSON.stringify(logs)).toContain('Debug message');
    });

    it('should log debug messages with context', () => {
      const logs: unknown[] = [];
      console.debug = (...args: unknown[]) => logs.push(args);

      logger.setLevel('debug');
      logger.debug('TestModule', 'Debug with context', { key: 'value', count: 42 });

      expect(logs.length).toBeGreaterThan(1); // Message + context
      expect(JSON.stringify(logs)).toContain('Context');
      expect(JSON.stringify(logs)).toContain('key');
    });

    it('should not log debug when level is info', () => {
      const logs: unknown[] = [];
      console.debug = (...args: unknown[]) => logs.push(args);

      logger.setLevel('info');
      logger.debug('TestModule', 'Should not appear');

      expect(logs.length).toBe(0);
    });

    it('should not log debug when level is warn', () => {
      const logs: unknown[] = [];
      console.debug = (...args: unknown[]) => logs.push(args);

      logger.setLevel('warn');
      logger.debug('TestModule', 'Should not appear');

      expect(logs.length).toBe(0);
    });

    it('should not log debug when level is error', () => {
      const logs: unknown[] = [];
      console.debug = (...args: unknown[]) => logs.push(args);

      logger.setLevel('error');
      logger.debug('TestModule', 'Should not appear');

      expect(logs.length).toBe(0);
    });

    it('should not log debug when level is silent', () => {
      const logs: unknown[] = [];
      console.debug = (...args: unknown[]) => logs.push(args);

      logger.setLevel('silent');
      logger.debug('TestModule', 'Should not appear');

      expect(logs.length).toBe(0);
    });
  });
});
