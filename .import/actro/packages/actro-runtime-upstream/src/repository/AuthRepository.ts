import { db } from '../libs/Sqlite';

export class AuthRepository {
  /**
   * 保存 Session Token
   */
  static async saveToken(env: string, token: string) {
    try {
      const sql = `
        INSERT OR REPLACE INTO auth (env, token, updated_at)
        VALUES (?, ?, ?)
      `;
      await db.executeSql(sql, [env, token, Date.now()]);
      console.log(`[AuthRepository] Token saved for env: ${env}`);
    } catch (error) {
      console.error('[AuthRepository] Failed to save token', error);
    }
  }

  /**
   * 获取 Session Token
   */
  static async getToken(env: string): Promise<string | null> {
    try {
      const rows = await db.selectAll<{ token: string }>(
        'SELECT token FROM auth WHERE env = ?',
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
   * 清除 Session Token
   */
  static async clearToken(env: string) {
    try {
      await db.executeSql('DELETE FROM auth WHERE env = ?', [env]);
      console.log(`[AuthRepository] Token cleared for env: ${env}`);
    } catch (error) {
      console.error('[AuthRepository] Failed to clear token', error);
    }
  }
}
