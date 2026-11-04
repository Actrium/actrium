/**
 * Remote Log Forwarder
 *
 * Will JS Log Pass HTTP POST Forward to developer actr log server.
 *
 * Host Parsing policy:
 *   From RN It's... scriptURL(Metro bundle - What's the name? IP,
 *   This would be automatic for simulators, real machines, or any platform, without manual configuration.
 *
 * Port is agreed:
 *   - iOS / macOS: 9999(with actr dev --log-server (Assisted)
 *   - Android: 9998(Independent) log server)
 *
 * Only __DEV__ The first step is to get the most of the most popular of the world's most popular.Release Build does not send any remote logs.
 */
import { Platform, NativeModules } from 'react-native';

const LOG_PORT = Platform.OS === 'android' ? 9998 : 9999;

/**
 * From Metro bundle It's... scriptURL Middle Ripper host.
 * scriptURL Like "http://localhost:8082/index.bundle?platform=ios&..."
 * The real machine, simulator, any platform applies.
 */
function resolveHost(): string {
  try {
    const sourceCode = NativeModules.SourceCode;
    const scriptURL: string | undefined =
      sourceCode?.getConstants?.()?.scriptURL ?? sourceCode?.scriptURL;
    if (scriptURL) {
      const match = scriptURL.match(/^https?:\/\/([^:/]+)/);
      if (match?.[1]) return match[1];
    }
  } catch {
    // ignore
  }
  // fallback: Simulator / Local development
  return 'localhost';
}

const LOG_SERVER_URL = `http://${resolveHost()}:${LOG_PORT}/log`;

function isDev(): boolean {
  return typeof (globalThis as any).__DEV__ === 'boolean'
    ? !!(globalThis as any).__DEV__
    : true;
}

/**
 * Send a log to remote log server(Whispering, silent failure)
 */
export function sendRemoteLog(level: string, msg: string): void {
  if (!isDev()) return;

  try {
    const fetchFn = (globalThis as any).fetch as unknown;
    if (typeof fetchFn !== 'function') return;

    (fetchFn as any)(LOG_SERVER_URL, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ level, msg }),
    })?.catch?.(() => {});
  } catch {
    // ignore
  }
}
