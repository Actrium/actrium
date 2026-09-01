import { logger } from './logger';

describe('AskitLogger', () => {
  let originalConsole: {
    error: typeof console.error;
    warn: typeof console.warn;
    log: typeof console.log;
  };
  let errorLogs: unknown[][] = [];
  let warnLogs: unknown[][] = [];
  let infoLogs: unknown[][] = [];

  beforeEach(() => {
    // Capture console output
    originalConsole = {
      error: console.error,
      warn: console.warn,
      log: console.log,
    };

    errorLogs = [];
    warnLogs = [];
    infoLogs = [];

    console.error = (...args) => errorLogs.push(args);
    console.warn = (...args) => warnLogs.push(args);
    console.log = (...args) => infoLogs.push(args);

    // Reset to default level
    logger.setLevel('info');
  });

  afterEach(() => {
    console.error = originalConsole.error;
    console.warn = originalConsole.warn;
    console.log = originalConsole.log;
    // Ensure logger level does not leak between tests
    logger.setLevel('info');
  });

  describe('setLevel and getLevel', () => {
    it('should set and get log level', () => {
      logger.setLevel('error');
      expect(logger.getLevel()).toBe('error');

      logger.setLevel('warn');
      expect(logger.getLevel()).toBe('warn');

      logger.setLevel('info');
      expect(logger.getLevel()).toBe('info');

      logger.setLevel('silent');
      expect(logger.getLevel()).toBe('silent');
    });
  });

  describe('error', () => {
    it('should log errors with module prefix', () => {
      logger.error('Test', 'Something went wrong');

      expect(errorLogs).toHaveLength(1);
      expect(errorLogs[0]).toEqual(['[askit/Test]', 'Something went wrong']);
    });

    it('should log errors with context', () => {
      logger.error('Test', 'Failed to load', {
        url: '/api/data',
        status: 500,
      });

      expect(errorLogs).toHaveLength(2);
      expect(errorLogs[0]).toEqual(['[askit/Test]', 'Failed to load']);
      expect(errorLogs[1]).toEqual(['[askit/Test]', 'Context:', { url: '/api/data', status: 500 }]);
    });

    it('should not log context if empty', () => {
      logger.error('Test', 'Error message', {});

      expect(errorLogs).toHaveLength(1);
      expect(errorLogs[0]).toEqual(['[askit/Test]', 'Error message']);
    });

    it('should respect silent level', () => {
      logger.setLevel('silent');
      logger.error('Test', 'This should not appear');

      expect(errorLogs).toHaveLength(0);
    });
  });

  describe('warn', () => {
    it('should log warnings with module prefix', () => {
      logger.warn('Test', 'Deprecated API');

      expect(warnLogs).toHaveLength(1);
      expect(warnLogs[0]).toEqual(['[askit/Test]', 'Deprecated API']);
    });

    it('should log warnings with context', () => {
      logger.warn('Test', 'Slow operation', {
        duration: 2500,
        threshold: 1000,
      });

      expect(warnLogs).toHaveLength(2);
      expect(warnLogs[0]).toEqual(['[askit/Test]', 'Slow operation']);
      expect(warnLogs[1]).toEqual([
        '[askit/Test]',
        'Context:',
        { duration: 2500, threshold: 1000 },
      ]);
    });

    it('should respect silent level', () => {
      logger.setLevel('silent');
      logger.warn('Test', 'This should not appear');

      expect(warnLogs).toHaveLength(0);
    });
  });

  describe('info', () => {
    it('should log info with module prefix', () => {
      logger.info('Test', 'Initialization complete');

      expect(infoLogs).toHaveLength(1);
      expect(infoLogs[0]).toEqual(['[askit/Test]', 'Initialization complete']);
    });

    it('should log info with context', () => {
      logger.info('Test', 'User logged in', {
        userId: 123,
        timestamp: 1234567890,
      });

      expect(infoLogs).toHaveLength(2);
      expect(infoLogs[0]).toEqual(['[askit/Test]', 'User logged in']);
      expect(infoLogs[1]).toEqual([
        '[askit/Test]',
        'Context:',
        { userId: 123, timestamp: 1234567890 },
      ]);
    });

    it('should respect error level (no info logs)', () => {
      logger.setLevel('error');
      logger.info('Test', 'This should not appear');

      expect(infoLogs).toHaveLength(0);
    });

    it('should respect warn level (no info logs)', () => {
      logger.setLevel('warn');
      logger.info('Test', 'This should not appear');

      expect(infoLogs).toHaveLength(0);
    });

    it('should respect silent level', () => {
      logger.setLevel('silent');
      logger.info('Test', 'This should not appear');

      expect(infoLogs).toHaveLength(0);
    });
  });

  describe('log level filtering', () => {
    it('error level should only show errors', () => {
      logger.setLevel('error');

      logger.error('Test', 'Error message');
      logger.warn('Test', 'Warning message');
      logger.info('Test', 'Info message');

      expect(errorLogs).toHaveLength(1);
      expect(warnLogs).toHaveLength(0);
      expect(infoLogs).toHaveLength(0);
    });

    it('warn level should show errors and warnings', () => {
      logger.setLevel('warn');

      logger.error('Test', 'Error message');
      logger.warn('Test', 'Warning message');
      logger.info('Test', 'Info message');

      expect(errorLogs).toHaveLength(1);
      expect(warnLogs).toHaveLength(1);
      expect(infoLogs).toHaveLength(0);
    });

    it('info level should show all logs', () => {
      logger.setLevel('info');

      logger.error('Test', 'Error message');
      logger.warn('Test', 'Warning message');
      logger.info('Test', 'Info message');

      expect(errorLogs).toHaveLength(1);
      expect(warnLogs).toHaveLength(1);
      expect(infoLogs).toHaveLength(1);
    });

    it('silent level should show nothing', () => {
      logger.setLevel('silent');

      logger.error('Test', 'Error message');
      logger.warn('Test', 'Warning message');
      logger.info('Test', 'Info message');

      expect(errorLogs).toHaveLength(0);
      expect(warnLogs).toHaveLength(0);
      expect(infoLogs).toHaveLength(0);
    });
  });
});
