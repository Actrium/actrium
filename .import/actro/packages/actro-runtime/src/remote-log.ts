/**
 * Remote Log Forwarder
 *
 * 将 JS 日志通过 HTTP POST 转发到开发机上的 actr log server。
 *
 * Host 解析策略：
 *   从 RN 的 scriptURL（Metro bundle 地址）中提取开发机 IP，
 *   这样无论是模拟器、真机、还是任何平台都能自动适配，无需手动配置。
 *
 * 端口约定：
 *   - iOS / macOS: 9999（与 actr dev --log-server 保持一致）
 *   - Android: 9998（独立 log server）
 *
 * 仅在 __DEV__ 模式下生效，Release 构建不会发送任何远程日志。
 */
import { Platform, NativeModules } from 'react-native';

const LOG_PORT = Platform.OS === 'android' ? 9998 : 9999;

/**
 * 从 Metro bundle 的 scriptURL 中提取开发机 host。
 * scriptURL 形如 "http://10.30.3.87:8082/index.bundle?platform=ios&..."
 * 真机、模拟器、任何平台都适用。
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
  // fallback: 模拟器 / 本地开发
  return 'localhost';
}

const LOG_SERVER_URL = `http://${resolveHost()}:${LOG_PORT}/log`;

function isDev(): boolean {
  return typeof (globalThis as any).__DEV__ === 'boolean'
    ? !!(globalThis as any).__DEV__
    : true;
}

/**
 * 发送一条日志到远程 log server（异步，静默失败）。
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
