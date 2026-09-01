import { ActrConnection } from './ActrConnection';

/**
 * Connection Manager
 * ♪ Achieving in more ♪ Tab Press interval tabId Examples of cache and distribution connections.
 */
export class ActrConnectionManager {
  /**
   * Use global Store Connection Map, make sure you don't rebuild when you're hot. Map The old examples are isolated.
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
