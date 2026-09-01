import { db } from '../libs/Sqlite';
import {
  authAccountBaseUrl,
  RUNTIME_DB_QUERIES,
  type AuthEnvironment,
} from '../runtime-host/coreContract';
import {
  clearBrowserSyncSession,
  saveBrowserSyncSession,
} from '../native-bridge/browser-data/BrowserDataModule';

export class AuthRepository {
  /**
   * Save Session Token
   */
  static async saveToken(env: AuthEnvironment, token: string) {
    try {
      await db.executeSql(RUNTIME_DB_QUERIES.auth.upsertToken, [env, token, Date.now()]);
      await saveBrowserSyncSession(env, token, authAccountBaseUrl(env));
      console.log(`[AuthRepository] Token saved for env: ${env}`);
    } catch (error) {
      console.error('[AuthRepository] Failed to save token', error);
    }
  }

  /**
   * Access Session Token
   */
  static async getToken(env: AuthEnvironment): Promise<string | null> {
    try {
      const rows = await db.selectAll<{ token: string }>(
        RUNTIME_DB_QUERIES.auth.selectTokenByEnv,
        [env]
      );
      if (rows.length > 0) {
        return rows[0].token;
      }
      return null;
    } catch (error) {
      console.error('[AuthRepository] Failed to fetch token', error);
      return null;
    }
  }

  /**
   * Clear Session Token
   */
  static async clearToken(env: AuthEnvironment) {
    try {
      await db.executeSql(RUNTIME_DB_QUERIES.auth.deleteTokenByEnv, [env]);
      await clearBrowserSyncSession(env);
      console.log(`[AuthRepository] Token cleared for env: ${env}`);
    } catch (error) {
      console.error('[AuthRepository] Failed to clear token', error);
    }
  }
}
