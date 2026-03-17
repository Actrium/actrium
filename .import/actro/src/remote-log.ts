/**
 * Remote Log Forwarder
 *
 * 将 JS 日志通过 HTTP POST 转发到开发机上的 actr log server。
 * 自动根据平台选择正确的 host 地址：
 *   - iOS / macOS: localhost（模拟器和宿主机共享网络栈）
 *   - Android 模拟器: 10.0.2.2（模拟器访问宿主机的特殊地址）
 *   - Android 真机: 需要通过 adb reverse 或手动配置局域网 IP
 *
 * 仅在 __DEV__ 模式下生效，Release 构建不会发送任何远程日志。
 */
import { Platform } from 'react-native';

// iOS/macOS 使用 9999（与 actr dev --log-server 保持一致）
// Android 使用 9998（独立 log server，避免端口冲突）
const LOG_PORT = Platform.OS === 'android' ? 9998 : 9999;

function resolveHost(): string {
  if (Platform.OS === 'android') {
    // Android 模拟器通过 10.0.2.2 访问宿主机 localhost。
    // 真机开发时请先执行: adb reverse tcp:9998 tcp:9998
    return '10.0.2.2';
  }
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
