import { NativeModules } from 'react-native';

export type LogLevel = 'trace' | 'debug' | 'info' | 'notice' | 'warn' | 'error' | 'fault';

type NativeLoggerModule = {
  log?: (message: string) => void;
  warn?: (message: string) => void;
  error?: (message: string) => void;
};

const nativeLogger: NativeLoggerModule | undefined = (NativeModules as any).NativeLogger;

function isDev(): boolean {
  // RN defines __DEV__ in most environments.
  return typeof (globalThis as any).__DEV__ === 'boolean' ? !!(globalThis as any).__DEV__ : true;
}

function format(level: LogLevel, category: string, msg: string): string {
  // Keep output aligned with Swift stderr mirroring: [LEVEL] [category] message
  return `[${level.toUpperCase()}] [${category}] ${msg}`;
}

function sendToNative(level: LogLevel, line: string): void {
  try {
    if (!nativeLogger) return;

    // Release policy: only forward notice+ to native.
    // (trace/debug/info remain local-only to reduce noise & protect potential PII.)
    if (!isDev() && (level === 'trace' || level === 'debug' || level === 'info')) return;

    // Native module supports log/warn/error. Map levels accordingly.
    if (level === 'error' || level === 'fault') nativeLogger.error?.(line);
    else if (level === 'warn') nativeLogger.warn?.(line);
    else nativeLogger.log?.(line);
  } catch {
    // ignore
  }
}

function sendToConsole(level: LogLevel, line: string): void {
  // Keep console output in dev for local debugging.
  if (!isDev()) return;
  try {
    if (level === 'error' || level === 'fault') console.error(line);
    else if (level === 'warn') console.warn(line);
    else if (level === 'debug' || level === 'trace') console.debug(line);
    else console.log(line);
  } catch {
    // ignore
  }
}

export class Logger {
  constructor(private readonly category: string) {}

  child(category: string): Logger {
    return new Logger(`${this.category}:${category}`);
  }

  private emit(level: LogLevel, msg: string): void {
    const line = format(level, this.category, msg);
    // Native is the primary sink so Swift can unify & optionally forward.
    sendToNative(level, line);
    sendToConsole(level, line);
  }

  trace(msg: string): void {
    this.emit('trace', msg);
  }
  debug(msg: string): void {
    this.emit('debug', msg);
  }
  info(msg: string): void {
    this.emit('info', msg);
  }
  notice(msg: string): void {
    this.emit('notice', msg);
  }
  warning(msg: string): void {
    this.emit('warn', msg);
  }
  error(msg: string): void {
    this.emit('error', msg);
  }
  fault(msg: string): void {
    this.emit('fault', msg);
  }
}

export const logger = new Logger('subsystem');
