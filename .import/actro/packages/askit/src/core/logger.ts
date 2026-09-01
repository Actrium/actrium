/**
 * Lightweight logging utility for askit
 * Provides consistent log formatting and configurable log levels
 *
 * Log levels (from least to most verbose):
 * - 'silent': No logs
 * - 'error': Only errors
 * - 'warn': Errors and warnings
 * - 'info': Errors, warnings, and info (default)
 * - 'debug': All logs including debug
 */

export type LogLevel = 'debug' | 'info' | 'warn' | 'error' | 'silent';
export type LogContext = Record<string, unknown>;

// Log level priority (lower = more verbose)
const LOG_PRIORITY: Record<LogLevel, number> = {
  debug: 0,
  info: 1,
  warn: 2,
  error: 3,
  silent: 4,
};

class AskitLogger {
  private level: LogLevel = 'info';

  /**
   * Set the minimum log level to display
   * - 'debug': All logs including debug
   * - 'info': Errors, warnings, and info (default)
   * - 'warn': Errors and warnings
   * - 'error': Only errors
   * - 'silent': No logs
   */
  setLevel(level: LogLevel): void {
    this.level = level;
  }

  /**
   * Get current log level
   */
  getLevel(): LogLevel {
    return this.level;
  }

  /**
   * Check if a log level should be displayed
   */
  private shouldLog(level: LogLevel): boolean {
    return LOG_PRIORITY[level] >= LOG_PRIORITY[this.level];
  }

  /**
   * Log a debug message with optional context
   */
  debug(module: string, message: string, context?: LogContext): void {
    if (!this.shouldLog('debug')) return;

    const prefix = `[askit/${module}]`;
    console.debug(prefix, message);
    if (context && Object.keys(context).length > 0) {
      console.debug(prefix, 'Context:', context);
    }
  }

  /**
   * Log an info message with optional context
   */
  info(module: string, message: string, context?: LogContext): void {
    if (!this.shouldLog('info')) return;

    const prefix = `[askit/${module}]`;
    console.log(prefix, message);
    if (context && Object.keys(context).length > 0) {
      console.log(prefix, 'Context:', context);
    }
  }

  /**
   * Log a warning with optional context
   */
  warn(module: string, message: string, context?: LogContext): void {
    if (!this.shouldLog('warn')) return;

    const prefix = `[askit/${module}]`;
    console.warn(prefix, message);
    if (context && Object.keys(context).length > 0) {
      console.warn(prefix, 'Context:', context);
    }
  }

  /**
   * Log an error with optional context
   */
  error(module: string, message: string, context?: LogContext): void {
    if (!this.shouldLog('error')) return;

    const prefix = `[askit/${module}]`;
    console.error(prefix, message);
    if (context && Object.keys(context).length > 0) {
      console.error(prefix, 'Context:', context);
    }
  }
}

/**
 * Shared logger instance
 */
export const logger = new AskitLogger();
