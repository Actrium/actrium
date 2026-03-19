import { ActrConnection } from './ActrConnection';

/**
 * 连接管理器
 * 实现在多 Tab 间按 tabId 缓存和分发连接实例。
 */
export class ActrConnectionManager {
  /**
   * 使用 global 存储连接 Map，确保热更新时不会重建 Map 导致旧实例被孤立。
   */
  private static get connections(): Map<string, ActrConnection> {
    if (!(global as any).__actrConnections) {
      (global as any).__actrConnections = new Map<string, ActrConnection>();
    }
    return (global as any).__actrConnections;
  }

  static getOrCreate({
    appId,
    actorType,
    tabId,
  }: {
    appId: string;
    actorType: string;
    tabId: string;
  }): ActrConnection {
    if (!appId || !tabId) {
      throw new Error('appId and tabId are required');
    }

    if (!this.connections.has(tabId)) {
      this.connections.set(
        tabId,
        new ActrConnection({ appId, actorType, tabId }),
      );
    }
    return this.connections.get(tabId)!;
  }

  static get(tabId: string): ActrConnection | undefined {
    return this.connections.get(tabId);
  }

  static async remove(tabId: string) {
    const conn = this.connections.get(tabId);
    if (conn) {
      await conn.disconnect();
      this.connections.delete(tabId);
    }
  }
}
